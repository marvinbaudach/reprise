//! GTK instrumental-fassungen UX (experimental) — the app-hosted stem-
//! separation worker host, the conversion/staging view, AI badges, the
//! wait-state, and the "Experimental features" gate (docs/ux-rules.md
//! Section AB; plan `docs/plans/multi-frontend-core.md` §2.4/3.2).
//!
//! ## Dependency boundary
//!
//! This module consumes the **reprise-core** facades — `ai_jobs`, `ai_staging`,
//! `ai_promotion`, `ai_conversion`, `provenance`, `stem_separation` (the trait +
//! the deterministic `FakeStemBackend`). The worker host is generic over the
//! `StemSeparationBackend` trait, so the **default build never links
//! reprise-stems** and runs the Fake (the enforced architecture probe + CI
//! check). P3b wires the real backend behind the **`stem-backend` cargo
//! feature**: the GTK app is a sanctioned binary host for reprise-stems
//! (LICENSING.md; `scripts/check-architecture.sh`), so under that feature
//! [`app_backend`] returns the real, lazily-provisioned `OrtStemBackend`.
//!
//! ## Progress numbers
//!
//! Every progress figure the UI shows comes from the `ai_jobs` rows/events
//! (`ai_jobs::batch_progress`, the row `progress_permille`) — the same numbers
//! the CLI and MCP report (plan §2.2). Nothing reads backend-internal state.

pub(in crate::ui) mod conversion_model;
pub(in crate::ui) mod conversion_view;
pub(in crate::ui) mod conversion_wiring;
pub(in crate::ui) mod worker_host;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use reprise_core::library::settings;
use reprise_core::stem_separation::StemSeparationBackend;
use rusqlite::Connection;

/// The persisted settings key gating **all** instrumental UI (INST-11,
/// Beschluss 11). A bespoke key rather than a `reprise_core::modules`
/// descriptor: this is a master gate over a whole surface (not a plain plugin
/// toggle), and the plan scopes it to the frontend's `ui/preferences`. Default
/// off.
pub(in crate::ui) const EXPERIMENTAL_ENABLED_KEY: &str = "experimental_features.enabled";

/// Whether the "Experimental features" master switch is on. Every instrumental
/// surface (context-menu entry, conversion view, AI badge, "Hide AI music"
/// filter) reads this before showing anything (INST-11 / FIL-7). Defaults to
/// `false` and tolerates a read error by staying hidden — the safe default for
/// an experimental gate.
pub(in crate::ui) fn experimental_enabled(conn: &Connection) -> bool {
    settings::get_bool(conn, EXPERIMENTAL_ENABLED_KEY, false).unwrap_or(false)
}

/// Persists the master switch. Wrapping `settings::set_bool` keeps the key a
/// single source of truth for both the preferences toggle and every reader.
pub(in crate::ui) fn set_experimental_enabled(
    conn: &Connection,
    enabled: bool,
) -> Result<(), rusqlite::Error> {
    settings::set_bool(conn, EXPERIMENTAL_ENABLED_KEY, enabled)
}

/// Resolves a job's `source_track_id` to the absolute source file path the
/// backend reads. Injected into the worker host so the render logic stays pure
/// and testable (the tests hand it a closure over a temp file).
///
/// It takes the worker's own `&Connection` so the production resolver
/// ([`db_source_resolver`]) can look the path up through the core facade
/// [`reprise_core::queries::track_source_path`] — no SQL in frontend code.
pub(in crate::ui) type SourceResolver =
    Arc<dyn Fn(&Connection, i64) -> Option<PathBuf> + Send + Sync>;

/// The stem-separation backend this app build runs. The default build uses the
/// deterministic [`reprise_core::stem_separation::FakeStemBackend`]; the
/// `stem-backend` feature swaps in the real, lazily-provisioned
/// `stem_backend::LazyOrtBackend`. The worker host is generic over the trait,
/// so only this constructor and [`app_model_id`] differ between builds.
#[cfg(not(feature = "stem-backend"))]
pub(in crate::ui) fn app_backend() -> Box<dyn StemSeparationBackend + Send> {
    Box::new(reprise_core::stem_separation::FakeStemBackend::new())
}

#[cfg(feature = "stem-backend")]
pub(in crate::ui) fn app_backend() -> Box<dyn StemSeparationBackend + Send> {
    Box::new(stem_backend::LazyOrtBackend::new())
}

/// The model id every enqueue stamps as the job's `params_fingerprint`. It must
/// match the id of the backend [`app_backend`] produces output with, so dedup
/// (Beschluss 16) and the `REPRISE_AI_MODEL` provenance tag stay consistent.
#[cfg(not(feature = "stem-backend"))]
pub(in crate::ui) fn app_model_id() -> String {
    reprise_core::stem_separation::FakeStemBackend::new().model_id()
}

#[cfg(feature = "stem-backend")]
pub(in crate::ui) fn app_model_id() -> String {
    reprise_stems::model::HTDEMUCS_FP32.model_id.to_string()
}

thread_local! {
    /// The UI-thread hook that nudges the worker to re-poll the queue. The
    /// conversion wiring registers it with the worker handle; the enqueue paths
    /// (context menu) call [`wake_worker`] after creating jobs so a freshly
    /// queued render starts without waiting for the next event. A thread-local
    /// keeps the worker handle out of unrelated widgets' state — everything here
    /// runs single-threaded on the UI thread.
    static WAKE_HOOK: RefCell<Option<Rc<dyn Fn()>>> = const { RefCell::new(None) };
}

/// Registers the worker-wake hook (called once by the conversion wiring).
pub(in crate::ui) fn set_wake_hook(hook: Rc<dyn Fn()>) {
    WAKE_HOOK.with(|hook_cell| *hook_cell.borrow_mut() = Some(hook));
}

/// Nudges the worker to re-poll the queue, if one is running. A no-op when the
/// experimental feature is off (no hook registered).
pub(in crate::ui) fn wake_worker() {
    let hook = WAKE_HOOK.with(|hook_cell| hook_cell.borrow().clone());
    if let Some(hook) = hook {
        hook();
    }
}

/// Unix seconds — the clock every facade call (`enqueue`, `promote`, `discard`)
/// on the UI thread feeds `ai_jobs`/`ai_promotion`.
pub(in crate::ui) fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

/// The production source-path resolver: looks the job's `source_track_id` up
/// through the core facade [`reprise_core::queries::track_source_path`]. A
/// missing row (or a query error) resolves to `None`, so the worker fails the
/// job with a clear `source-unavailable` error rather than rendering from a
/// bogus path.
pub(in crate::ui) fn db_source_resolver() -> SourceResolver {
    Arc::new(
        |conn: &Connection, source_track_id: i64| -> Option<PathBuf> {
            match reprise_core::queries::track_source_path(conn, source_track_id) {
                Ok(path) => path,
                Err(error) => {
                    tracing::error!(%error, source_track_id, "instrumental: source path lookup failed");
                    None
                }
            }
        },
    )
}

/// The real stem-separation backend, behind the `stem-backend` feature so the
/// default build never links reprise-stems.
#[cfg(feature = "stem-backend")]
mod stem_backend {
    use std::cell::RefCell;
    use std::path::Path;

    use reprise_core::stem_separation::{ProgressPermille, StemError, StemSeparationBackend};
    use reprise_stems::OrtStemBackend;

    /// The real `OrtStemBackend`, constructed **lazily from a provisioned model
    /// on first render** — it never downloads inline, so app launch never blocks
    /// on a 316 MB fetch. Until the model is provisioned, each render fails with
    /// a clear `StemError::Backend` (the job's normal failure path, and the
    /// worker's panic/failure guard keeps the thread alive); the feature stays
    /// usable and a render after the model is downloaded picks it up. A single
    /// worker thread owns the backend and drives one job at a time, so a
    /// `RefCell` (Send, not Sync — like `OrtStemBackend` itself) is enough.
    pub(super) struct LazyOrtBackend {
        inner: RefCell<Option<OrtStemBackend>>,
    }

    impl LazyOrtBackend {
        pub(super) fn new() -> Self {
            Self {
                inner: RefCell::new(None),
            }
        }
    }

    impl StemSeparationBackend for LazyOrtBackend {
        fn separate_instrumental(
            &self,
            source: &Path,
            output: &Path,
            progress: &mut dyn FnMut(ProgressPermille),
            cancel: &dyn Fn() -> bool,
        ) -> Result<(), StemError> {
            if self.inner.borrow().is_none() {
                match OrtStemBackend::from_provisioned_default()? {
                    Some(backend) => *self.inner.borrow_mut() = Some(backend),
                    None => {
                        return Err(StemError::Backend(
                            "the stem-separation model is not provisioned yet; \
                             download it to enable instrumental rendering"
                                .to_string(),
                        ))
                    }
                }
            }
            let guard = self.inner.borrow();
            guard
                .as_ref()
                .expect("constructed above")
                .separate_instrumental(source, output, progress, cancel)
        }

        fn model_id(&self) -> String {
            reprise_stems::model::HTDEMUCS_FP32.model_id.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_source_resolver_resolves_a_seeded_track_and_none_for_a_missing_one() {
        // P3b wiring: the production resolver now returns the real source path
        // (it used to be a hard-coded None), so the app-hosted worker can render.
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at, file_mtime, file_size) \
             VALUES (1, '/music/x.flac', 'X', 'A', 1, 1, 1)",
            [],
        )
        .unwrap();

        let resolve = db_source_resolver();
        assert_eq!(resolve(&conn, 1), Some(PathBuf::from("/music/x.flac")));
        assert_eq!(
            resolve(&conn, 999),
            None,
            "a missing track resolves to None"
        );
    }
}
