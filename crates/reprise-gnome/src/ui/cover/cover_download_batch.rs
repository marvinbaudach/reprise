use std::cell::Cell;
use std::path::PathBuf;
use std::rc::{Rc, Weak};

use gtk4::glib;
use reprise_core::db::Db;
use reprise_core::library::startup_tasks::{self, SignatureTask};

use super::cover_download_worker::{CoverDownloadRuntime, DownloadOutcome, DownloadRequest};
use super::player_controller::PlayerController;
use super::progress_subscribers::ProgressSubscribers;
use super::track_list::TrackList;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum BatchState {
    Idle,
    Running,
    Complete,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) struct BatchProgress {
    pub(in crate::ui) state: BatchState,
    pub(in crate::ui) checked: usize,
    pub(in crate::ui) total: usize,
    pub(in crate::ui) downloaded: usize,
    pub(in crate::ui) unavailable: usize,
}

impl BatchProgress {
    fn idle() -> Self {
        Self {
            state: BatchState::Idle,
            checked: 0,
            total: 0,
            downloaded: 0,
            unavailable: 0,
        }
    }

    fn running(total: usize) -> Self {
        Self {
            state: if total == 0 {
                BatchState::Complete
            } else {
                BatchState::Running
            },
            total,
            ..Self::idle()
        }
    }

    fn failed() -> Self {
        Self {
            state: BatchState::Failed,
            ..Self::idle()
        }
    }

    fn advance(mut self, outcome: &DownloadOutcome) -> Self {
        if self.state != BatchState::Running {
            return self;
        }
        self.checked = self.checked.saturating_add(1).min(self.total);
        match outcome {
            DownloadOutcome::AlreadyCovered => {}
            DownloadOutcome::Downloaded(_) => self.downloaded += 1,
            DownloadOutcome::Unavailable => self.unavailable += 1,
            DownloadOutcome::TransientFailure => {}
        }
        if self.checked == self.total {
            self.state = BatchState::Complete;
        }
        self
    }

    /// The run is through: every open track has been answered.
    ///
    /// Without this the only way out of `Running` is `checked` meeting
    /// `total` exactly. That holds for a run that asks about every track it
    /// counted, but not for one whose settled tracks were filtered out after
    /// the count — and a run that can never reach its own total leaves the
    /// card showing work that will never happen, with no way to dismiss it.
    fn completed(mut self) -> Self {
        if self.state == BatchState::Running {
            self.state = BatchState::Complete;
        }
        self
    }

    pub(in crate::ui) fn fraction(self) -> f64 {
        if self.total == 0 {
            return f64::from(self.state == BatchState::Complete);
        }
        self.checked as f64 / self.total as f64
    }
}

pub(in crate::ui) struct CoverDownloadBatch {
    conn: Rc<Db>,
    runtime: CoverDownloadRuntime,
    track_list: Rc<TrackList>,
    player: Option<Rc<PlayerController>>,
    generation: Cell<u64>,
    running: Cell<bool>,
    progress: Cell<BatchProgress>,
    progress_subscribers: ProgressSubscribers<BatchProgress>,
}

impl CoverDownloadBatch {
    pub(in crate::ui) fn new(
        conn: &Rc<Db>,
        runtime: &CoverDownloadRuntime,
        track_list: &Rc<TrackList>,
        player: Option<&Rc<PlayerController>>,
    ) -> Rc<Self> {
        Rc::new(Self {
            conn: conn.clone(),
            runtime: runtime.clone(),
            track_list: track_list.clone(),
            player: player.cloned(),
            generation: Cell::new(0),
            running: Cell::new(false),
            progress: Cell::new(BatchProgress::idle()),
            progress_subscribers: ProgressSubscribers::default(),
        })
    }

    pub(in crate::ui) fn subscribe_progress(
        &self,
        is_alive: impl Fn() -> bool + 'static,
        callback: impl Fn(BatchProgress) + 'static,
    ) {
        self.progress_subscribers
            .subscribe(self.progress.get(), is_alive, callback);
    }

    pub(in crate::ui) fn start(self: &Rc<Self>) {
        if !self.runtime.enabled.get() {
            self.set_progress(BatchProgress::idle());
            return;
        }
        let Some(pass) = startup_tasks::begin_exact(&self.conn, SignatureTask::CoverDownload)
        else {
            self.set_progress(BatchProgress::running(0));
            return;
        };
        self.start_pass(pass);
    }

    /// Starts a pass requested by the user even when startup freshness says
    /// the library is already settled. A second request joins the active pass
    /// instead of replacing it with overlapping work.
    pub(in crate::ui) fn start_user_triggered(self: &Rc<Self>) {
        if self.running.get() {
            return;
        }
        if !self.runtime.enabled.get() {
            self.set_progress(BatchProgress::idle());
            return;
        }
        let pass = startup_tasks::begin_user_triggered(&self.conn, SignatureTask::CoverDownload);
        self.start_pass(pass);
    }

    fn start_pass(self: &Rc<Self>, pass: startup_tasks::ExactTaskPass) {
        let paths = {
            let conn = &self.conn;
            reprise_core::queries::query_live_track_paths(conn)
        };
        let paths = match paths {
            Ok(paths) => paths,
            Err(error) => {
                tracing::warn!(%error, "could not query tracks for cover download");
                self.set_progress(BatchProgress::failed());
                return;
            }
        };
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        self.running.set(true);
        if paths.is_empty() {
            // A run of nothing is a run that is already done — `running(0)`
            // says exactly that, and the batches waiting on this one need to
            // hear it.
            self.set_progress(BatchProgress::running(0));
            pass.record_completed_or_warn(&self.conn);
            self.running.set(false);
            return;
        }

        let this = self.clone();
        glib::spawn_future_local(async move {
            let _active_run = ActiveRun::new(&this, generation);
            // Every track the download side has already settled is dropped
            // here, before anything opens a file. Asking the worker means it
            // reads the track's tags to work out which album to look up, and
            // this batch runs over the whole library on every launch — so
            // without this filter a settled library re-reads every file it
            // owns, every time, to be told what it was told last time.
            let paths = {
                let candidates = paths.clone();
                gtk4::gio::spawn_blocking(move || {
                    open_paths(candidates, |path| {
                        reprise_core::cover::download_marked_unavailable(
                            std::path::Path::new(path),
                            reprise_core::cover::ThumbnailSize::List,
                        )
                    })
                })
                .await
                .unwrap_or(paths)
            };
            if this.generation.get() != generation {
                return;
            }
            // Only now is the run's size known. Announcing the library's size
            // before the filter ran promised work this run will never do: on a
            // settled library every track drops out here, and the card would
            // sit at "0 of <library>" for the rest of the session.
            this.set_progress(BatchProgress::running(paths.len()));
            if paths.is_empty() {
                pass.record_completed_or_warn(&this.conn);
                return;
            }

            for path in &paths {
                if this.generation.get() != generation {
                    return;
                }
                let (response, result) = async_channel::bounded(1);
                if !this
                    .runtime
                    .request(DownloadRequest {
                        track_path: path.clone(),
                        skip_if_covered: true,
                        response,
                    })
                    .await
                {
                    let progress = if this.runtime.enabled.get() {
                        BatchProgress::failed()
                    } else {
                        BatchProgress::idle()
                    };
                    this.set_progress(progress);
                    return;
                }
                let outcome = result
                    .recv()
                    .await
                    .unwrap_or(DownloadOutcome::TransientFailure);
                if this.generation.get() != generation {
                    return;
                }
                // Only definitive results settle the track. Transport and
                // worker failures stay open so a later pass can retry them.
                if outcome_settles_track(&outcome) {
                    let settled = path.clone();
                    gtk4::gio::spawn_blocking(move || {
                        reprise_core::cover::remember_download_unavailable(
                            std::path::Path::new(&settled),
                            reprise_core::cover::ThumbnailSize::List,
                        );
                    })
                    .await
                    .ok();
                }
                this.set_progress(this.progress.get().advance(&outcome));
            }

            if this.generation.get() != generation {
                return;
            }
            this.set_progress(this.progress.get().completed());
            pass.record_completed_or_warn(&this.conn);
            let refreshed_paths: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
            this.track_list.reload();
            if let Some(player) = &this.player {
                player.refresh_edited_cover(&refreshed_paths);
            }
        });
    }

    /// Stops the run in flight and clears the card.
    ///
    /// The generation bump is the stop signal the spawned task already checks
    /// after every await; the idle progress is what takes the card away. A
    /// later run — a finished scan, say — is free to start again.
    pub(in crate::ui) fn cancel(&self) {
        self.generation.set(self.generation.get().wrapping_add(1));
        self.running.set(false);
        self.set_progress(BatchProgress::idle());
    }

    fn set_progress(&self, progress: BatchProgress) {
        self.progress.set(progress);
        self.progress_subscribers.notify(progress);
    }

    #[cfg(test)]
    pub(in crate::ui) fn set_progress_for_test(&self, progress: BatchProgress) {
        self.set_progress(progress);
    }

    #[cfg(test)]
    pub(in crate::ui) fn progress_for_test(&self) -> BatchProgress {
        self.progress.get()
    }

    #[cfg(test)]
    pub(in crate::ui) fn generation_for_test(&self) -> u64 {
        self.generation.get()
    }
}

struct ActiveRun {
    batch: Weak<CoverDownloadBatch>,
    generation: u64,
}

impl ActiveRun {
    fn new(batch: &Rc<CoverDownloadBatch>, generation: u64) -> Self {
        Self {
            batch: Rc::downgrade(batch),
            generation,
        }
    }
}

impl Drop for ActiveRun {
    fn drop(&mut self) {
        let Some(batch) = self.batch.upgrade() else {
            return;
        };
        if batch.generation.get() == self.generation {
            batch.running.set(false);
        }
    }
}

/// The tracks this run still has to ask the download side about.
///
/// A track the download side has already settled is not an open item, so it
/// is neither asked about again nor counted — the run's size is the number of
/// open items, never the size of the library.
fn open_paths(paths: Vec<String>, is_settled: impl Fn(&str) -> bool) -> Vec<String> {
    paths.into_iter().filter(|path| !is_settled(path)).collect()
}

fn outcome_settles_track(outcome: &DownloadOutcome) -> bool {
    matches!(
        outcome,
        DownloadOutcome::AlreadyCovered | DownloadOutcome::Unavailable
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{open_paths, outcome_settles_track, BatchProgress, BatchState};
    use crate::ui::cover_download_worker::DownloadOutcome;

    #[test]
    fn progress_counts_checked_downloaded_and_unavailable_outcomes() {
        let progress = BatchProgress::running(3)
            .advance(&DownloadOutcome::AlreadyCovered)
            .advance(&DownloadOutcome::Downloaded(PathBuf::from(
                "/cache/cover.jpg",
            )))
            .advance(&DownloadOutcome::Unavailable);

        assert_eq!(progress.state, BatchState::Complete);
        assert_eq!(progress.checked, 3);
        assert_eq!(progress.downloaded, 1);
        assert_eq!(progress.unavailable, 1);
        assert_eq!(progress.fraction(), 1.0);
    }

    #[test]
    fn only_definitive_cover_outcomes_settle_a_track() {
        assert!(outcome_settles_track(&DownloadOutcome::AlreadyCovered));
        assert!(outcome_settles_track(&DownloadOutcome::Unavailable));
        assert!(!outcome_settles_track(&DownloadOutcome::TransientFailure));
        assert!(!outcome_settles_track(&DownloadOutcome::Downloaded(
            PathBuf::from("/cache/cover.jpg")
        )));
    }

    #[test]
    fn a_run_is_sized_by_the_tracks_still_open_not_by_the_whole_library() {
        let library = vec![
            "/music/settled.flac".to_string(),
            "/music/open.flac".to_string(),
            "/music/also-settled.flac".to_string(),
        ];

        let open = open_paths(library, |path| path != "/music/open.flac");

        assert_eq!(
            open,
            vec!["/music/open.flac".to_string()],
            "only the tracks the download side has not settled are open items"
        );
    }

    #[test]
    fn a_settled_library_finishes_instead_of_running_forever() {
        let library = vec![
            "/music/a.flac".to_string(),
            "/music/b.flac".to_string(),
            "/music/c.flac".to_string(),
        ];

        let open = open_paths(library, |_| true);
        let progress = BatchProgress::running(open.len());

        assert_eq!(
            progress.state,
            BatchState::Complete,
            "a run with nothing left to check is done, not stuck at 0 of the library"
        );
    }

    #[test]
    fn a_run_ends_when_its_open_tracks_are_through() {
        // The filter left two of the library's tracks open. Once both have
        // been answered the run is over — nothing else will ever arrive.
        let progress = BatchProgress::running(2)
            .advance(&DownloadOutcome::AlreadyCovered)
            .advance(&DownloadOutcome::Unavailable)
            .completed();

        assert_eq!(progress.state, BatchState::Complete);
        assert_eq!(progress.fraction(), 1.0);
    }

    #[test]
    fn a_run_cut_short_still_ends_rather_than_hanging_at_its_last_count() {
        // Defensive: whatever stopped the loop early, the card must not be
        // left showing a run that can no longer move.
        let progress = BatchProgress::running(4)
            .advance(&DownloadOutcome::Downloaded(PathBuf::from("/cache/a.jpg")))
            .completed();

        assert_eq!(progress.state, BatchState::Complete);
        assert_eq!(progress.downloaded, 1);
    }

    #[test]
    fn live_track_paths_excludes_missing_rows_and_is_stable() {
        let conn = crate::test_db::open().unwrap();
        crate::test_db::connection(&conn)
            .execute(
                "INSERT INTO tracks (path, title, added_at) \
             VALUES ('/music/b.mp3', 'B', 1)",
                [],
            )
            .unwrap();
        crate::test_db::connection(&conn)
            .execute(
                "INSERT INTO tracks (path, title, added_at) \
             VALUES ('/music/a.mp3', 'A', 1)",
                [],
            )
            .unwrap();
        crate::test_db::connection(&conn)
            .execute(
                "INSERT INTO tracks (path, title, added_at, missing_since) \
             VALUES ('/music/gone.mp3', 'Gone', 1, 1)",
                [],
            )
            .unwrap();

        assert_eq!(
            reprise_core::queries::query_live_track_paths(&conn).unwrap(),
            vec!["/music/a.mp3".to_string(), "/music/b.mp3".to_string()]
        );
    }
}
