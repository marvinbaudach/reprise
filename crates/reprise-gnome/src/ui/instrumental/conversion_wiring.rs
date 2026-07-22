//! Window-side wiring for the instrumental slice (INST-6/INST-7): constructs
//! the worker host and the conversion view, drives the view off the worker's
//! coalesced progress ticks, and connects the save/discard/save-all/clear
//! affordances to the `ai_promotion`/`ai_jobs` core facades. Play of a staging
//! render is a P3b concern (no play-by-path yet), so it is a clearly-marked stub.
//!
//! Everything here is gated on the experimental switch (INST-11): with the
//! switch off, no worker thread starts and no view page is added.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::ai_promotion::{self, PromotionConfig};
use reprise_core::ai_staging::StagingStore;
use reprise_core::{ai_jobs, library::settings};
use rusqlite::Connection;

use super::conversion_view::ConversionView;
use super::worker_host::InstrumentalWorker;
use crate::ui::strings;
use crate::ui::track_list::TrackList;

/// The `content_stack` page name the conversion view is added under. Reachable
/// selection from the sidebar is cross-package integration (a `ViewSource` /
/// sidebar entry live in core + non-owned files) and is tracked for P3b; the
/// page is added here so the view is live and progress-refreshed.
const CONVERSION_PAGE: &str = "conversions";

const CLEAR_RESPONSE_DISCARD: &str = "discard-all";
const CLEAR_RESPONSE_CANCEL: &str = "cancel";

/// The refs the wiring borrows from the window's `RuntimeWiring` bundle.
pub(in crate::ui) struct ConversionWiring<'a> {
    pub conn: &'a Rc<RefCell<Connection>>,
    pub db_path: &'a Path,
    pub window: &'a adw::ApplicationWindow,
    pub content_stack: &'a gtk4::Stack,
    pub toast_overlay: &'a adw::ToastOverlay,
    pub track_list: &'a Rc<TrackList>,
}

/// Starts the worker host + conversion view when the experimental switch is on.
pub(in crate::ui) fn install(deps: &ConversionWiring<'_>) {
    if !super::experimental_enabled(&deps.conn.borrow()) {
        return;
    }
    let staging = StagingStore::with_default_dir();
    if let Err(error) = staging.ensure_dir() {
        tracing::warn!(%error, "instrumental: could not create staging dir");
    }

    let worker = InstrumentalWorker::new(
        deps.db_path.to_path_buf(),
        super::app_backend(),
        staging.clone(),
        super::db_source_resolver(),
        std::process::id() as i64,
    );
    // The enqueue paths (context menu) nudge the worker through this hook so a
    // freshly queued render starts immediately rather than after the next event.
    super::set_wake_hook({
        let worker = worker.clone();
        Rc::new(move || worker.wake())
    });

    let view = ConversionView::new(deps.conn.clone(), staging.clone());
    deps.content_stack
        .add_named(view.widget(), Some(CONVERSION_PAGE));

    wire_callbacks(&view, &staging, deps);

    // Progress is not a change_log event (plan §2.2), so the worker's coalesced
    // tick is how the aggregate bar stays live. Drop-safe: when the worker is
    // dropped its sender closes and this future ends. The same tick also drives
    // the library refresh when the worker auto-promotes a render: that write is
    // the app's own (filtered from the external-changes runtime), so nothing else
    // reloads the track list — watch the saved-job count and reload when it grows.
    let receiver = worker.progress_receiver();
    let view_weak = Rc::downgrade(&view);
    let refresh_conn = deps.conn.clone();
    let track_list_weak = Rc::downgrade(deps.track_list);
    let saved_baseline = std::cell::Cell::new(saved_job_count(&refresh_conn));
    glib::spawn_future_local(async move {
        while receiver.recv().await.is_ok() {
            if let Some(view) = view_weak.upgrade() {
                view.refresh();
            }
            let saved_now = saved_job_count(&refresh_conn);
            if saved_now > saved_baseline.get() {
                saved_baseline.set(saved_now);
                if let Some(track_list) = track_list_weak.upgrade() {
                    track_list.reload();
                }
            }
        }
    });

    // The close handler owns the sole strong refs to the worker and view, so
    // both live exactly as long as the window; the worker thread is joined on
    // close (its Drop is the backstop).
    deps.window.connect_close_request(move |_| {
        let _keep_view_alive = &view;
        worker.shutdown();
        glib::Propagation::Proceed
    });
}

fn wire_callbacks(view: &Rc<ConversionView>, staging: &StagingStore, deps: &ConversionWiring<'_>) {
    let overlay = deps.toast_overlay.downgrade();

    // Save (INST-6): promote the staged render into the library, then refresh
    // the view and the track list so the new instrumental appears.
    {
        let conn = deps.conn.clone();
        let staging = staging.clone();
        let view_weak = Rc::downgrade(view);
        let track_list = Rc::downgrade(deps.track_list);
        let overlay = overlay.clone();
        view.set_on_save(move |job_id| {
            let message = match promote_one(&conn, &staging, job_id) {
                Ok(()) => {
                    if let Some(view) = view_weak.upgrade() {
                        view.refresh();
                    }
                    if let Some(track_list) = track_list.upgrade() {
                        track_list.reload();
                    }
                    strings::text(strings::STATE_SAVED)
                }
                Err(message) => message,
            };
            toast(&overlay, &message);
        });
    }

    // Discard (INST-6): drop the staging render; the row leaves the view.
    {
        let conn = deps.conn.clone();
        let staging = staging.clone();
        let view_weak = Rc::downgrade(view);
        view.set_on_discard(move |job_id| {
            let now = super::now_unix();
            let outcome = ai_jobs::discard_staged(&conn.borrow(), &staging, job_id, now);
            if let Err(error) = outcome {
                tracing::error!(%error, job_id, "instrumental: discard failed");
            }
            if let Some(view) = view_weak.upgrade() {
                view.refresh();
            }
        });
    }

    // Save all (INST-6): promote every undecided render.
    {
        let conn = deps.conn.clone();
        let staging = staging.clone();
        let view_weak = Rc::downgrade(view);
        let track_list = Rc::downgrade(deps.track_list);
        let overlay = overlay.clone();
        view.set_on_save_all(move || {
            let undecided = undecided_job_ids(&conn);
            let mut saved = 0usize;
            for job_id in undecided {
                if promote_one(&conn, &staging, job_id).is_ok() {
                    saved += 1;
                }
            }
            if let Some(view) = view_weak.upgrade() {
                view.refresh();
            }
            if let Some(track_list) = track_list.upgrade() {
                track_list.reload();
            }
            toast(&overlay, &strings::conversion_aggregate(saved, saved, 100));
        });
    }

    // Clear (INST-7): warn when undecided renders exist before discarding them.
    {
        let conn = deps.conn.clone();
        let staging = staging.clone();
        let view_weak = Rc::downgrade(view);
        let window = deps.window.clone();
        view.set_on_clear(move || {
            let has_undecided = view_weak
                .upgrade()
                .is_some_and(|view| view.has_undecided_now());
            if has_undecided {
                confirm_clear(&window, &conn, &staging, &view_weak);
            } else {
                clear_undecided(&conn, &staging, &view_weak);
            }
        });
    }

    // Play (INST-4/INST-5): playing a staging render needs a play-by-path seam
    // the player does not have yet. TODO(P3b): wire staging/library playback.
    {
        let overlay = overlay.clone();
        view.set_on_play(move |job_id| {
            tracing::info!(job_id, "instrumental: play requested (not yet wired — P3b)");
            toast(&overlay, &strings::text(strings::STATE_PROCESSING));
        });
    }
}

/// Reads the library root, builds the promotion config, and promotes one job.
/// Returns a user-facing message on failure.
fn promote_one(
    conn: &Rc<RefCell<Connection>>,
    staging: &StagingStore,
    job_id: i64,
) -> Result<(), String> {
    let library_root = {
        let conn = conn.borrow();
        settings::get_library_root(&conn).ok().flatten()
    };
    let Some(library_root) = library_root else {
        return Err("Set a library folder before saving instrumentals".to_string());
    };
    let config = PromotionConfig::new(library_root);
    let now = super::now_unix();
    let mut guard = conn.borrow_mut();
    ai_promotion::promote(&mut guard, staging, &config, job_id, now)
        .map(|_| ())
        .map_err(|error| {
            tracing::error!(%error, job_id, "instrumental: promotion failed");
            format!("Could not save instrumental: {error}")
        })
}

/// The number of worker-promoted (saved) renders, via the core facade — the
/// signal the progress future watches to reload the library after an app-hosted
/// auto-promotion. A read error reads as `0` (no growth => no spurious reload).
fn saved_job_count(conn: &Rc<RefCell<Connection>>) -> i64 {
    reprise_core::ai_jobs::count_saved(&conn.borrow()).unwrap_or(0)
}

/// The ids of every finished, unsaved render currently in the view.
fn undecided_job_ids(conn: &Rc<RefCell<Connection>>) -> Vec<i64> {
    let conn = conn.borrow();
    ai_jobs::list_active_jobs(&conn)
        .unwrap_or_default()
        .into_iter()
        .filter(|job| job.state == ai_jobs::JobState::Done && job.result_track_id.is_none())
        .map(|job| job.id)
        .collect()
}

fn clear_undecided(
    conn: &Rc<RefCell<Connection>>,
    staging: &StagingStore,
    view_weak: &std::rc::Weak<ConversionView>,
) {
    let now = super::now_unix();
    for job_id in undecided_job_ids(conn) {
        let _ = ai_jobs::discard_staged(&conn.borrow(), staging, job_id, now);
    }
    if let Some(view) = view_weak.upgrade() {
        view.refresh();
    }
}

fn confirm_clear(
    window: &adw::ApplicationWindow,
    conn: &Rc<RefCell<Connection>>,
    staging: &StagingStore,
    view_weak: &std::rc::Weak<ConversionView>,
) {
    let alert = adw::AlertDialog::builder()
        .heading(strings::text(strings::CONVERSION_CLEAR))
        .body(strings::text(strings::STATE_READY_UNSAVED))
        .build();
    alert.add_response(
        CLEAR_RESPONSE_CANCEL,
        &strings::text(strings::CONVERSION_CLEAR),
    );
    alert.add_response(
        CLEAR_RESPONSE_DISCARD,
        &strings::text(strings::CONVERSION_DISCARD),
    );
    alert.set_response_appearance(CLEAR_RESPONSE_DISCARD, adw::ResponseAppearance::Destructive);
    alert.set_default_response(Some(CLEAR_RESPONSE_CANCEL));
    alert.set_close_response(CLEAR_RESPONSE_CANCEL);

    let conn = conn.clone();
    let staging = staging.clone();
    let view_weak = view_weak.clone();
    alert.connect_response(None, move |_, response| {
        if response == CLEAR_RESPONSE_DISCARD {
            clear_undecided(&conn, &staging, &view_weak);
        }
    });
    alert.present(Some(window));
}

fn toast(overlay: &glib::WeakRef<adw::ToastOverlay>, message: &str) {
    if let Some(overlay) = overlay.upgrade() {
        overlay.add_toast(adw::Toast::new(message));
    }
}
