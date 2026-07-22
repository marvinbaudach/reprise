//! Window-side wiring for the instrumental slice (INST-6/INST-7): constructs
//! the worker host and the conversion view, drives the view off the worker's
//! coalesced progress ticks, and connects the save/discard/save-all/clear
//! affordances to the `ai_promotion`/`ai_jobs` core facades, and plays a
//! finished render — a staging file by path, a promoted render as a library
//! track (INST-4b/5b).
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
use crate::ui::player_controller::PlayerController;
use crate::ui::strings;
use crate::ui::track_list::TrackList;

/// The `content_stack` page name the conversion view is added under. The
/// sidebar's `ViewSource::Conversions` row (INST-13, added under the same
/// experimental gate) selects this page; the page is added here so the view is
/// live and progress-refreshed. Must match `library_shell`'s Conversions branch.
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
    /// The player, so a finished render can actually play by path (INST-4b).
    /// `None` in headless builds without a player.
    pub player: &'a Option<Rc<PlayerController>>,
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

    // Play (INST-4b/INST-5b): a finished, undecided render plays its staging
    // file by path; a promoted render plays through the normal library path; a
    // still-processing row does not play at all (wait-with-progress, INST-5b —
    // its Play affordance is already disabled by the view, this is the backstop).
    {
        let conn = deps.conn.clone();
        let staging = staging.clone();
        let player = deps.player.as_ref().map(Rc::downgrade);
        let overlay = overlay.clone();
        view.set_on_play(move |job_id| {
            let Some(player) = player.as_ref().and_then(std::rc::Weak::upgrade) else {
                return;
            };
            match play_target(&conn, &staging, job_id) {
                Some(PlayTarget::LibraryTrack(track_id)) => player.play_track_id(track_id),
                Some(PlayTarget::StagingPath(path)) => match path.to_str() {
                    Some(path) => {
                        // A staging render is not a library track, so it plays as
                        // a first-class one-off PREVIEW through the controller
                        // (INST-4b): the controller parks the gapless pre-feed,
                        // suspends queue-advance-on-finish, credits no play, and
                        // reflects a marked preview state — never the raw
                        // `player.play` bypass that let a stale pre-feed hand off
                        // into an unrelated queue track.
                        let (title, artist) = preview_labels(&conn, job_id);
                        if let Err(error) = player.play_preview(path, &title, &artist) {
                            tracing::warn!(%error, job_id, "instrumental: staging preview failed");
                            toast(&overlay, &strings::text(strings::STATE_FAILED));
                        }
                    }
                    None => {
                        tracing::warn!(job_id, "instrumental: staging path is not valid UTF-8");
                    }
                },
                // Queued / processing / failed: no play (INST-5b wait-with-progress).
                None => {}
            }
        });
    }
}

/// What activating a conversion row plays (INST-4b). A saved render is a real
/// library track; an undecided render is a staging file played by path.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PlayTarget {
    /// A promoted render: play the library track by id (full now-playing).
    LibraryTrack(i64),
    /// An undecided staging render: play the file by absolute path.
    StagingPath(std::path::PathBuf),
}

/// Resolves what a conversion row should play, or `None` when it must not play
/// (queued/processing/failed → wait-with-progress, INST-5b; done-but-discarded →
/// nothing). Pure over the DB + staging store, so INST-4b/5b are testable
/// without a player.
fn play_target(
    conn: &Rc<RefCell<Connection>>,
    staging: &StagingStore,
    job_id: i64,
) -> Option<PlayTarget> {
    let job = ai_jobs::get_job(&conn.borrow(), job_id).ok().flatten()?;
    if job.state != ai_jobs::JobState::Done {
        return None; // still queued/processing, or failed/cancelled — no play.
    }
    if let Some(track_id) = job.result_track_id {
        return Some(PlayTarget::LibraryTrack(track_id)); // saved -> library track.
    }
    staging
        .exists(job_id)
        .then(|| PlayTarget::StagingPath(staging.path_for_job(job_id)))
}

/// Resolves the source track's `(title, artist)` for a preview's marked
/// now-playing label (INST-4b), or empty strings when the job has no source or
/// the row is gone — `play_preview` then falls back to a plain "Instrumental
/// preview" title. Kept out of `play_target` so that pure resolver stays
/// player-free and testable.
fn preview_labels(conn: &Rc<RefCell<Connection>>, job_id: i64) -> (String, String) {
    let conn = conn.borrow();
    let Some(source_id) = ai_jobs::get_job(&conn, job_id)
        .ok()
        .flatten()
        .and_then(|job| job.source_track_id)
    else {
        return (String::new(), String::new());
    };
    reprise_core::queries::query_track_summary(&conn, source_id)
        .ok()
        .flatten()
        .map(|summary| (summary.title, summary.artist))
        .unwrap_or_default()
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

#[cfg(test)]
mod tests {
    use super::{play_target, PlayTarget};
    use reprise_core::ai_jobs;
    use reprise_core::ai_staging::StagingStore;
    use rusqlite::Connection;
    use std::cell::RefCell;
    use std::rc::Rc;

    const WORKER: i64 = 7;
    const NOW: i64 = 100;

    fn setup() -> (Rc<RefCell<Connection>>, StagingStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let conn = Connection::open_in_memory().unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) \
             VALUES (1, '/m/1.flac', 'S', 'A', 0)",
            [],
        )
        .unwrap();
        let staging = StagingStore::new(dir.path().join("staging"));
        staging.ensure_dir().unwrap();
        (Rc::new(RefCell::new(conn)), staging, dir)
    }

    fn enqueue(conn: &Rc<RefCell<Connection>>, staging: &StagingStore, model: &str) -> i64 {
        ai_jobs::enqueue_instrumental(&conn.borrow(), staging, 1, model, NOW)
            .unwrap()
            .job_id()
    }

    // UX INST-4b: activating a finished render resolves to its file for playback —
    // an undecided render plays its staging file, a saved render its library track.
    #[test]
    fn inst_4b_finished_render_resolves_to_its_file_for_playback() {
        let (conn, staging, _dir) = setup();
        let job = enqueue(&conn, &staging, "model@1");
        ai_jobs::claim_next(&conn.borrow(), WORKER, NOW, 1000)
            .unwrap()
            .unwrap();
        ai_jobs::mark_done(&conn.borrow(), job, WORKER, NOW).unwrap();
        std::fs::write(staging.path_for_job(job), b"render").unwrap();

        assert_eq!(
            play_target(&conn, &staging, job),
            Some(PlayTarget::StagingPath(staging.path_for_job(job))),
            "an undecided render plays its staging file by path"
        );
        // The staging render plays as a one-off PREVIEW, not a queue track: the
        // wiring routes a `StagingPath` through `PlayerController::play_preview`,
        // whose preview mode stops (never advances the queue) when it finishes —
        // so a stale gapless pre-feed can't hand off into an unrelated track and
        // no play is credited to the wrong one.
        assert!(
            !crate::ui::playback::preview::PlaybackMode::Preview.advances_queue_on_finish(),
            "a finished instrumental preview must not advance the queue"
        );

        // Once promoted (a result track id is set), it plays the library track.
        conn.borrow()
            .execute(
                "UPDATE ai_jobs SET result_track_id = 1 WHERE id = ?1",
                [job],
            )
            .unwrap();
        assert_eq!(
            play_target(&conn, &staging, job),
            Some(PlayTarget::LibraryTrack(1)),
            "a saved render plays its promoted library track"
        );
    }

    // UX INST-5b: a still-processing (or queued) row never resolves a play target
    // — the wait-with-progress rule: no play, no original fallback, no skip.
    #[test]
    fn inst_5b_a_processing_or_queued_row_never_plays() {
        let (conn, staging, _dir) = setup();
        let running = enqueue(&conn, &staging, "model@1");
        ai_jobs::claim_next(&conn.borrow(), WORKER, NOW, 1000)
            .unwrap()
            .unwrap();
        ai_jobs::set_progress(&conn.borrow(), running, WORKER, 400).unwrap();
        assert_eq!(
            play_target(&conn, &staging, running),
            None,
            "a processing row does not play"
        );

        let queued = enqueue(&conn, &staging, "model@2");
        assert_eq!(
            play_target(&conn, &staging, queued),
            None,
            "a queued row does not play"
        );
    }
}
