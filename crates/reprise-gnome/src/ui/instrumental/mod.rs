//! GTK instrumental-fassungen UX (experimental) — the out-of-process worker
//! supervisor, conversion/staging view, AI badges, wait-state, and the
//! "Experimental features" gate (docs/ux-rules.md Section AB; plan
//! `docs/plans/multi-frontend-core.md` §2.4/3.2).
//!
//! ## Dependency boundary
//!
//! This module consumes only **reprise-core** facades. The native ONNX Runtime
//! stack lives in the separately packaged `reprise-worker` executable. The
//! `stem-backend` feature enables the GTK client surface and model provisioner,
//! then embeds the worker's libexec path; it never links the `ort` inference
//! feature into the music-player process.
//!
//! ## Progress numbers
//!
//! Every progress figure the UI shows comes from the `ai_jobs` rows/events
//! (`ai_jobs::batch_progress`, the row `progress_permille`) — the same numbers
//! the CLI and MCP report (plan §2.2). Nothing reads backend-internal state.

pub(in crate::ui) mod conversion_model;
pub(in crate::ui) mod conversion_view;
pub(in crate::ui) mod conversion_wiring;
mod runtime;
pub(in crate::ui) mod worker_host;

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use reprise_core::ai_jobs::EnqueueOutcome;
use reprise_core::ai_staging::StagingStore;
use reprise_core::library::settings;
use rusqlite::Connection;

type EnabledHook = Rc<dyn Fn(bool)>;

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

/// Whether this build contains the packaged production worker client.
///
/// This is deliberately a compile-time capability: a normal build must not
/// expose a conversion action that can silently fall back to test behavior.
pub(in crate::ui) const fn production_backend_compiled() -> bool {
    cfg!(feature = "stem-backend")
}

/// The model id every enqueue stamps as the job's `params_fingerprint`. It must
/// match the worker backend, so dedup (Beschluss 16) and the
/// `REPRISE_AI_MODEL` provenance tag stay consistent. Core owns the stable
/// cross-process identity; the GTK client never imports the backend crate.
#[cfg(not(feature = "stem-backend"))]
pub(in crate::ui) fn app_model_id() -> Option<String> {
    None
}

#[cfg(feature = "stem-backend")]
pub(in crate::ui) fn app_model_id() -> Option<String> {
    Some(reprise_core::stem_separation::CURRENT_MODEL_ID.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) struct EnqueueSummary {
    pub created: usize,
    pub deduplicated: usize,
    pub skipped_unavailable: usize,
}

impl EnqueueSummary {
    pub(in crate::ui) fn accepted(self) -> usize {
        self.created + self.deduplicated
    }
}

#[derive(Debug, thiserror::Error)]
#[cfg_attr(not(feature = "stem-backend"), allow(dead_code))]
pub(in crate::ui) enum EnqueueError {
    #[error("this build has no production instrumental backend")]
    BackendUnavailable,
    #[error("the instrumental model must be downloaded first")]
    ModelRequired,
    #[error("the packaged instrumental runtime is unavailable: {0}")]
    RuntimeUnavailable(String),
    #[error(transparent)]
    Database(#[from] rusqlite::Error),
}

/// Enqueues the present subset of a drag/selection through the same batch and
/// dedup facade. Missing, removed, repeated, and unknown ids never become jobs.
pub(in crate::ui) fn enqueue_present_tracks(
    conn: &Connection,
    ids: &[i64],
) -> Result<EnqueueSummary, EnqueueError> {
    if ids.is_empty() {
        return Ok(EnqueueSummary {
            created: 0,
            deduplicated: 0,
            skipped_unavailable: 0,
        });
    }
    #[cfg(feature = "stem-backend")]
    match reprise_stems::provision::runtime_readiness() {
        reprise_stems::provision::RuntimeReadiness::Ready(_) => {}
        reprise_stems::provision::RuntimeReadiness::ModelRequired { .. } => {
            return Err(EnqueueError::ModelRequired);
        }
        reprise_stems::provision::RuntimeReadiness::Unavailable { detail, .. } => {
            return Err(EnqueueError::RuntimeUnavailable(detail));
        }
    }
    let model = app_model_id().ok_or(EnqueueError::BackendUnavailable)?;
    enqueue_present_tracks_with_model(
        conn,
        &StagingStore::with_default_dir(),
        ids,
        &model,
        now_unix(),
    )
    .map_err(EnqueueError::Database)
}

fn enqueue_present_tracks_with_model(
    conn: &Connection,
    staging: &StagingStore,
    ids: &[i64],
    model: &str,
    now: i64,
) -> Result<EnqueueSummary, rusqlite::Error> {
    let unique_requested = ids.iter().copied().collect::<HashSet<_>>().len();
    let present = reprise_core::queries::filter_present(conn, ids)?;
    let skipped_unavailable = unique_requested.saturating_sub(present.len());
    if present.is_empty() {
        return Ok(EnqueueSummary {
            created: 0,
            deduplicated: 0,
            skipped_unavailable,
        });
    }
    let batch = reprise_core::ai_conversion::add_batch_to_conversion(
        conn, staging, &present, model, false, now,
    )?;
    let created = batch
        .jobs
        .iter()
        .filter(|outcome| matches!(outcome, EnqueueOutcome::Created { .. }))
        .count();
    Ok(EnqueueSummary {
        created,
        deduplicated: batch.jobs.len() - created,
        skipped_unavailable,
    })
}

thread_local! {
    /// The UI-thread hook that nudges the worker to re-poll the queue. The
    /// conversion wiring registers it with the worker handle; the enqueue paths
    /// (context menu) call [`wake_worker`] after creating jobs so a freshly
    /// queued render starts without waiting for the next event. A thread-local
    /// keeps the worker handle out of unrelated widgets' state — everything here
    /// runs single-threaded on the UI thread.
    static WAKE_HOOK: RefCell<Option<Rc<dyn Fn()>>> = const { RefCell::new(None) };
    /// Window-runtime hook for applying the persisted master gate immediately.
    /// Preferences owns persistence; conversion wiring owns process/UI lifetime.
    static ENABLED_HOOK: RefCell<Option<EnabledHook>> = const { RefCell::new(None) };
    /// Opens the Experimental preferences page when first-use assets need work.
    static SETTINGS_HOOK: RefCell<Option<Rc<dyn Fn()>>> = const { RefCell::new(None) };
}

/// Registers the worker-wake hook (called once by the conversion wiring).
pub(in crate::ui) fn set_wake_hook(hook: Rc<dyn Fn()>) {
    WAKE_HOOK.with(|hook_cell| *hook_cell.borrow_mut() = Some(hook));
}

/// Removes the wake target before a live disable drops its supervisor.
pub(in crate::ui) fn clear_wake_hook() {
    WAKE_HOOK.with(|hook_cell| hook_cell.borrow_mut().take());
}

/// Nudges the worker to re-poll the queue, if one is running. A no-op when the
/// experimental feature is off (no hook registered).
pub(in crate::ui) fn wake_worker() {
    let hook = WAKE_HOOK.with(|hook_cell| hook_cell.borrow().clone());
    if let Some(hook) = hook {
        hook();
    }
}

/// Registers the current window's live master-gate handler.
fn set_enabled_hook(hook: Rc<dyn Fn(bool)>) {
    ENABLED_HOOK.with(|hook_cell| *hook_cell.borrow_mut() = Some(hook));
}

/// Applies a persisted master-gate transition to the running window.
pub(in crate::ui) fn apply_enabled(enabled: bool) {
    let hook = ENABLED_HOOK.with(|hook_cell| hook_cell.borrow().clone());
    if let Some(hook) = hook {
        hook(enabled);
    }
}

/// Drops the current window's live gate handler during teardown.
fn clear_enabled_hook() {
    ENABLED_HOOK.with(|hook_cell| hook_cell.borrow_mut().take());
}

pub(in crate::ui) fn set_settings_hook(hook: Rc<dyn Fn()>) {
    SETTINGS_HOOK.with(|hook_cell| *hook_cell.borrow_mut() = Some(hook));
}

pub(in crate::ui) fn open_settings() {
    let hook = SETTINGS_HOOK.with(|hook_cell| hook_cell.borrow().clone());
    if let Some(hook) = hook {
        hook();
    }
}

fn clear_settings_hook() {
    SETTINGS_HOOK.with(|hook_cell| hook_cell.borrow_mut().take());
}

/// Unix seconds — the clock every facade call (`enqueue`, `promote`, `discard`)
/// on the UI thread feeds `ai_jobs`/`ai_promotion`.
pub(in crate::ui) fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(feature = "stem-backend"))]
    fn user_build_without_stem_backend_exposes_no_model() {
        assert_eq!(
            app_model_id(),
            None,
            "a user build without a production backend cannot stamp a fake model identity"
        );
    }

    #[test]
    fn live_toggle_hook_observes_every_runtime_transition() {
        let transitions = Rc::new(RefCell::new(Vec::new()));
        set_enabled_hook({
            let transitions = transitions.clone();
            Rc::new(move |enabled| transitions.borrow_mut().push(enabled))
        });

        apply_enabled(true);
        apply_enabled(false);

        assert_eq!(
            *transitions.borrow(),
            vec![true, false],
            "the running window must receive both live feature transitions"
        );
        clear_enabled_hook();
    }

    #[test]
    fn disabling_clears_the_worker_wake_target() {
        let wakes = Rc::new(std::cell::Cell::new(0));
        set_wake_hook({
            let wakes = wakes.clone();
            Rc::new(move || wakes.set(wakes.get() + 1))
        });

        wake_worker();
        clear_wake_hook();
        wake_worker();

        assert_eq!(
            wakes.get(),
            1,
            "queued work must not reach a stopped supervisor after live disable"
        );
    }

    #[test]
    fn conversion_drop_filters_missing_tracks_and_reuses_batch_dedup() {
        let conn = Connection::open_in_memory().unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) VALUES
             (1, '/music/live.flac', 'Live', 'Artist', 0),
             (2, '/music/missing.flac', 'Missing', 'Artist', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE tracks SET missing_since = 1, missing_reason = 'deleted' WHERE id = 2",
            [],
        )
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let staging = reprise_core::ai_staging::StagingStore::new(dir.path());

        let first =
            enqueue_present_tracks_with_model(&conn, &staging, &[1, 2, 1], "model@1", 10).unwrap();
        assert_eq!(
            first,
            EnqueueSummary {
                created: 1,
                deduplicated: 0,
                skipped_unavailable: 1,
            }
        );

        let second =
            enqueue_present_tracks_with_model(&conn, &staging, &[1, 2], "model@1", 20).unwrap();
        assert_eq!(
            second,
            EnqueueSummary {
                created: 0,
                deduplicated: 1,
                skipped_unavailable: 1,
            },
            "a second drop references the existing job instead of duplicating it"
        );
    }
}
