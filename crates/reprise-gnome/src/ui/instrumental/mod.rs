//! GTK instrumental-fassungen UX (experimental) — the app-hosted stem-
//! separation worker host, the conversion/staging view, AI badges, the
//! wait-state, and the "Experimental features" gate (docs/ux-rules.md
//! Section AB; plan `docs/plans/multi-frontend-core.md` §2.4/3.2).
//!
//! ## Dependency boundary (HARD CONSTRAINT)
//!
//! This module consumes only the **reprise-core** facades — `ai_jobs`,
//! `ai_staging`, `ai_promotion`, `ai_conversion`, `provenance`,
//! `stem_separation` (the trait + the deterministic `FakeStemBackend`). It
//! never depends on `reprise-stems`: the worker host is generic over the
//! `StemSeparationBackend` trait and, in this package, is instantiated with the
//! Fake. The real backend is wired behind the experimental switch in a small
//! P3b commit — see [`app_backend`]'s `TODO(P3b)`.
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
use reprise_core::stem_separation::{FakeStemBackend, StemSeparationBackend};
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
/// It takes the worker's own `&Connection` so a production resolver can look
/// the path up through a core facade. **Core-facade gap (reported):** there is
/// no by-id `track path` facade today (only the reverse,
/// `queries::track_id_for_path`), and productive gnome code must not assemble
/// SQL. So the production resolver ([`db_source_resolver`]) returns `None`
/// until P3b adds that facade and wires the real path lookup — the same P3b
/// commit that swaps in the real backend.
pub(in crate::ui) type SourceResolver =
    Arc<dyn Fn(&Connection, i64) -> Option<PathBuf> + Send + Sync>;

/// The stem-separation backend this app build runs.
///
/// **TODO(P3b):** package F must not depend on `reprise-stems` (HARD
/// CONSTRAINT), so the production worker is instantiated with the deterministic
/// [`FakeStemBackend`] from reprise-core. P3b swaps in the real reprise-stems
/// backend here (behind the experimental switch) in a small isolated commit.
/// Because the worker host is generic over the trait, only this one
/// constructor and [`app_model_id`] change.
pub(in crate::ui) fn app_backend() -> Box<dyn StemSeparationBackend + Send> {
    Box::new(FakeStemBackend::new())
}

/// The model id every enqueue stamps as the job's `params_fingerprint`. It must
/// match the id of the backend [`app_backend`] produces output with, so dedup
/// (Beschluss 16) and the `REPRISE_AI_MODEL` provenance tag stay consistent.
///
/// **TODO(P3b):** tracks the real backend's id once wired.
pub(in crate::ui) fn app_model_id() -> String {
    FakeStemBackend::new().model_id()
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

/// The production source-path resolver. **TODO(P3b):** returns `None` until a
/// by-id track-path core facade exists (see [`SourceResolver`]); a worker
/// claiming a job then fails it with a clear `source-unavailable` error rather
/// than rendering from a bogus path. The Fake backend already rejects a missing
/// source, so this stays honest end to end.
pub(in crate::ui) fn db_source_resolver() -> SourceResolver {
    Arc::new(
        |_conn: &Connection, _source_track_id: i64| -> Option<PathBuf> {
            // TODO(P3b): resolve through a core facade, e.g.
            // `queries::track_source_path(conn, source_track_id)`.
            None
        },
    )
}
