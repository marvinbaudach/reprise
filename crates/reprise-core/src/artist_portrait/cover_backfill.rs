//! Serial, cancellable library-wide album-cover backfill (B3).
//!
//! Rides inside the same artist-artwork backfill handle as
//! [`super::backfill::PortraitBackfill`], as a **separate run with its own
//! handle**: its own worker thread, its own `Db::open_ready` on the worker,
//! its own cancel flag and listener. `backfill.rs` (the portrait engine) is
//! read, not edited — its `launch` engine is not threaded with a second
//! worklist. The FFI chains the two: it starts this run once the portrait
//! run it already watches reports `Complete`.
//!
//! One album at a time, a representative track per album: whether that
//! album needs a network request at all is entirely the injected `fetch`
//! closure's call (decision 9, "only albums whose local resolution finds
//! nothing") — this module has no filesystem/SAF access to check that
//! itself, so it treats every album the same way, network or not.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use crate::db::Db;
use crate::queries::{self, WindowRange};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CoverBackfillProgress {
    pub done: u32,
    pub total: u32,
}

/// `(album_artist, album, representative_track_uri)`.
pub type CoverBackfillFetch =
    dyn Fn(&str, &str, &str) -> crate::cover_download::CoverFetchOutcome + Send + Sync;
pub type CoverBackfillListener = dyn Fn(CoverBackfillProgress) + Send + Sync;

type PrepareWork = dyn FnOnce() -> Result<Vec<(String, String, String)>, String> + Send;
type ConsentAllowed = dyn Fn() -> bool + Send + Sync;

struct Shared {
    active: bool,
    cancelled: bool,
    /// Sticky, unlike `cancelled`: set by [`CoverBackfill::cancel`] even
    /// while no run is active yet, and consumed by the next
    /// [`CoverBackfill::launch`] rather than by the (possibly nonexistent)
    /// worker `cancelled` gates. Exists for the window between the portrait
    /// run reporting `Complete` and the FFI's forwarding closure actually
    /// reaching `start()` — during that window `active` is `false` on both
    /// sides, so a plain `active`-gated cancel is silently lost (B3 review
    /// findings 6/7).
    cancel_requested: bool,
    progress: CoverBackfillProgress,
    listener: Option<Arc<CoverBackfillListener>>,
}

/// Owns the sole worker thread and its latest immutable progress snapshot —
/// the same shape as [`super::backfill::PortraitBackfill`], minus the retry
/// queue: a cover fetch's own pacing and retry-worthiness live in
/// `cover_download`, not here.
pub struct CoverBackfill {
    shared: Arc<Mutex<Shared>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl CoverBackfill {
    #[must_use]
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Mutex::new(Shared {
                active: false,
                cancelled: false,
                cancel_requested: false,
                progress: CoverBackfillProgress::default(),
                listener: None,
            })),
            worker: Mutex::new(None),
        }
    }

    /// Starts a run whose worklist (one representative track per album) is
    /// read on the worker from its own `Db`, opened from `database_path`.
    pub fn start(
        &self,
        database_path: PathBuf,
        fetch: Arc<CoverBackfillFetch>,
        listener: Arc<CoverBackfillListener>,
        consent_allowed: Arc<ConsentAllowed>,
    ) -> bool {
        self.launch(
            Box::new(move || {
                let db = Db::open_ready(&database_path).map_err(|error| error.to_string())?;
                pending_albums(&db).map_err(|error| error.to_string())
            }),
            fetch,
            listener,
            consent_allowed,
        )
    }

    pub fn cancel(&self) {
        let listener = {
            let mut shared = self
                .shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !shared.active {
                // No run to stop yet — but one may still be about to start
                // (the FFI's forwarding closure calls `start()` after this
                // handle reports the portrait run `Complete`, not before),
                // so the cancel must not simply vanish: the next `launch`
                // consumes this instead of starting.
                shared.cancel_requested = true;
                return;
            }
            shared.cancelled = true;
            shared.progress = CoverBackfillProgress::default();
            shared.listener.take()
        };
        if let Some(listener) = listener {
            listener(CoverBackfillProgress::default());
        }
    }

    /// Resets the handle to idle for a test harness that shares this one
    /// process-global instance across many cases
    /// (`crates/reprise-android-ffi/src/artist_portrait/album_cover.rs`'s
    /// `reset_album_cover_state_for_tests`): stops any run still active,
    /// the same as `cancel`, and — unlike `cancel`, which is deliberately
    /// sticky — also drops a pending cancel a previous case may have left
    /// set, so it cannot block the next case's `start`. `cancel`'s own
    /// early return for an already-finished run leaves `progress` at
    /// whatever that run last reported — correct for `cancel` itself,
    /// which a caller only reaches while a run is still meaningfully
    /// active, but wrong here: a previous case's completed run must not
    /// leak its `done`/`total` into the next case's fresh state.
    pub fn reset_for_tests(&self) {
        self.cancel();
        let mut shared = self
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        shared.cancel_requested = false;
        shared.progress = CoverBackfillProgress::default();
    }

    #[must_use]
    pub fn progress(&self) -> CoverBackfillProgress {
        self.shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .progress
    }

    fn launch(
        &self,
        prepare: Box<PrepareWork>,
        fetch: Arc<CoverBackfillFetch>,
        listener: Arc<CoverBackfillListener>,
        consent_allowed: Arc<ConsentAllowed>,
    ) -> bool {
        let mut worker = self
            .worker
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(previous) = worker.take() {
            if previous.is_finished() {
                if previous.join().is_err() {
                    tracing::error!("album cover backfill worker panicked");
                    reset_after_worker_exit(&self.shared);
                }
            } else {
                self.shared
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .listener = Some(listener);
                *worker = Some(previous);
                return false;
            }
        }

        {
            let mut shared = self
                .shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if shared.active {
                shared.listener = Some(listener);
                return false;
            }
            if shared.cancel_requested {
                // A cancel arrived while this run existed only as an
                // in-flight FFI closure, before `start()` reached here —
                // honour it instead of starting a pass the caller already
                // tried to stop (B3 review findings 6/7). One-shot: an
                // unrelated later `start()` is not blocked by it.
                shared.cancel_requested = false;
                return false;
            }
            shared.active = true;
            shared.cancelled = false;
            shared.progress = CoverBackfillProgress::default();
            shared.listener = Some(Arc::clone(&listener));
        }

        let shared = Arc::clone(&self.shared);
        *worker = Some(std::thread::spawn(move || {
            let albums = match prepare() {
                Ok(albums) => albums,
                Err(error) => {
                    tracing::warn!(%error, "album cover backfill could not prepare its worklist");
                    finish_without_run(&shared);
                    return;
                }
            };
            run_worker(&shared, albums, fetch.as_ref(), consent_allowed.as_ref());
        }));
        true
    }

    #[cfg(test)]
    fn start_prepared(
        &self,
        albums: Vec<(String, String, String)>,
        fetch: Arc<CoverBackfillFetch>,
        listener: Arc<CoverBackfillListener>,
        consent_allowed: Arc<ConsentAllowed>,
    ) -> bool {
        self.launch(
            Box::new(move || Ok(albums)),
            fetch,
            listener,
            consent_allowed,
        )
    }
}

impl Default for CoverBackfill {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for CoverBackfill {
    fn drop(&mut self) {
        self.cancel();
        if let Some(worker) = self
            .worker
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            if worker.join().is_err() {
                tracing::error!("album cover backfill worker panicked while shutting down");
            }
        }
    }
}

/// One `(album_artist, album, representative_track_uri)` per album in the
/// library, in the same paged read `PortraitBackfill::pending_artists` uses.
fn pending_albums(db: &Db) -> Result<Vec<(String, String, String)>, rusqlite::Error> {
    let mut albums = Vec::new();
    let mut offset = 0_i64;
    loop {
        let window = queries::query_albums(
            db,
            "",
            WindowRange {
                offset,
                limit: i64::MAX,
            },
        )?;
        let returned = window.rows.len();
        albums.extend(
            window
                .rows
                .into_iter()
                .map(|album| (album.album_artist, album.album, album.representative_path)),
        );
        if !window.has_more || returned == 0 {
            return Ok(albums);
        }
        offset = offset.saturating_add(i64::try_from(returned).unwrap_or(i64::MAX));
    }
}

fn cancelled(shared: &Mutex<Shared>) -> bool {
    shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .cancelled
}

fn run_worker(
    shared: &Mutex<Shared>,
    albums: Vec<(String, String, String)>,
    fetch: &CoverBackfillFetch,
    consent_allowed: &ConsentAllowed,
) {
    let total = u32::try_from(albums.len()).unwrap_or(u32::MAX);
    let mut progress = CoverBackfillProgress { done: 0, total };
    if !publish(shared, progress) {
        return finish_cancelled(shared);
    }
    for (album_artist, album, representative_uri) in albums {
        if cancelled(shared) {
            return finish_cancelled(shared);
        }
        if !consent_allowed() {
            return finish_cancelled(shared);
        }
        let _ = fetch(&album_artist, &album, &representative_uri);
        progress.done = progress.done.saturating_add(1);
        if !publish(shared, progress) {
            return finish_cancelled(shared);
        }
    }
    finish(shared, progress);
}

fn publish(shared: &Mutex<Shared>, progress: CoverBackfillProgress) -> bool {
    let listener = {
        let mut shared = shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if shared.cancelled {
            return false;
        }
        shared.progress = progress;
        shared.listener.clone()
    };
    if let Some(listener) = listener {
        listener(progress);
    }
    true
}

fn finish(shared: &Mutex<Shared>, progress: CoverBackfillProgress) {
    let listener = {
        let mut shared = shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if shared.cancelled {
            shared.active = false;
            return;
        }
        shared.progress = progress;
        shared.active = false;
        shared.listener.clone()
    };
    if let Some(listener) = listener {
        listener(progress);
    }
}

fn finish_cancelled(shared: &Mutex<Shared>) {
    let listener = {
        let mut shared = shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        shared.active = false;
        shared.cancelled = true;
        shared.progress = CoverBackfillProgress::default();
        shared.listener.take()
    };
    if let Some(listener) = listener {
        listener(CoverBackfillProgress::default());
    }
}

fn finish_without_run(shared: &Mutex<Shared>) {
    // Notifies the listener the same way `finish`/`finish_cancelled` do:
    // the FFI's forwarding closure (`artist_portrait.rs`) may have just
    // pushed a `Running` update in anticipation of this pass actually
    // running (B3 review finding 1) — if `prepare()` fails and nothing
    // ever follows that push, the listener must still hear about it, or
    // Kotlin is left showing a spinner for a pass that silently never
    // started.
    let listener = {
        let mut shared = shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        shared.active = false;
        shared.progress = CoverBackfillProgress::default();
        shared.listener.take()
    };
    if let Some(listener) = listener {
        listener(CoverBackfillProgress::default());
    }
}

fn reset_after_worker_exit(shared: &Mutex<Shared>) {
    let mut shared = shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    shared.active = false;
    shared.cancelled = false;
    shared.progress = CoverBackfillProgress::default();
    shared.listener = None;
}

#[cfg(test)]
#[path = "cover_backfill_tests.rs"]
mod tests;
