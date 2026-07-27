//! Window-side wiring for the instrumental slice (INST-6/INST-7): constructs
//! the worker-process supervisor and the conversion view, drives the view off
//! the supervisor's coalesced progress ticks, and connects save/discard/save-all/clear
//! affordances to the `ai_promotion`/`ai_jobs` core facades, and plays a
//! finished render — a staging file by path, a promoted render as a library
//! track (INST-4b/5b).
//!
//! Everything here is gated on the experimental switch (INST-11): with the
//! switch off, no worker process starts and no view page is added.

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
use crate::ui::player_controller::PlayerController;
use crate::ui::strings;
use crate::ui::track_list::TrackList;

/// The `content_stack` page name the conversion view is added under. The
/// sidebar's `ViewSource::Conversions` row (INST-13) selects this page. The page
/// is installed under the **same** experimental gate as the row — either up
/// front by [`install`] (switch on at construction) or on demand by
/// [`ensure_page_installed`] the moment the row is selected after a live
/// toggle-on — so the row can never select a missing page. Must match
/// `library_shell`'s Conversions branch.
pub(super) const CONVERSION_PAGE: &str = "conversions";

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

/// Installs the window-owned live runtime. It applies the persisted gate now
/// and every later preference transition without restarting the application.
pub(in crate::ui) fn install(deps: &ConversionWiring<'_>) {
    super::runtime::install(deps);
}

thread_local! {
    /// INST-13 router seam: the idempotent hook that installs the conversions
    /// content page on demand. Registered once by [`install`] — even when the
    /// experimental switch starts off — so a later toggle-on + row selection
    /// routes through [`ensure_page_installed`] and lands on a real page instead
    /// of a missing one. The hook owns the lazily-created view, keeping it alive
    /// for the window's life.
    static ENSURE_PAGE_HOOK: RefCell<Option<Rc<dyn Fn()>>> = const { RefCell::new(None) };
}

/// Ensures the conversions content page exists before the sidebar router selects
/// it (INST-13). Called by `library_shell`'s `ViewSource::Conversions` branch
/// ahead of `set_visible_child_name`, so the reviewer's live sequence — switch
/// on → sidebar rebuild shows the row → click — installs the page under the same
/// experimental gate and never selects a missing one. Idempotent and gated; a
/// no-op when the switch is off, the page already exists, or the window is gone.
pub(in crate::ui) fn ensure_page_installed() {
    let hook = ENSURE_PAGE_HOOK.with(|hook| hook.borrow().clone());
    if let Some(hook) = hook {
        hook();
    }
}

pub(super) fn set_ensure_page_hook(hook: Rc<dyn Fn()>) {
    ENSURE_PAGE_HOOK.with(|cell| *cell.borrow_mut() = Some(hook));
}

pub(super) fn clear_ensure_page_hook() {
    ENSURE_PAGE_HOOK.with(|cell| cell.borrow_mut().take());
}

/// Idempotently installs the conversions content page (INST-13), returning the
/// view when it created one. Adds the [`ConversionView`] page under
/// [`CONVERSION_PAGE`] only while the experimental switch is on — the **same**
/// gate that reveals the sidebar row — and only when the page is absent, so
/// neither the up-front nor the on-demand path can add a duplicate. `None` (a
/// no-op) when the switch is off or the page already exists.
pub(super) fn install_conversions_page(
    content_stack: &gtk4::Stack,
    conn: &Rc<RefCell<Connection>>,
    staging: &StagingStore,
) -> Option<Rc<ConversionView>> {
    if !super::experimental_enabled(&conn.borrow()) {
        return None;
    }
    if content_stack.child_by_name(CONVERSION_PAGE).is_some() {
        return None;
    }
    let view = ConversionView::new(conn.clone(), staging.clone());
    content_stack.add_named(view.widget(), Some(CONVERSION_PAGE));
    Some(view)
}

/// Removes the gated surface and leaves it safely when it was selected.
pub(super) fn remove_conversions_page(content_stack: &gtk4::Stack) {
    if content_stack.visible_child_name().as_deref() == Some(CONVERSION_PAGE) {
        crate::ui::window::content_stack::show_page(content_stack, "library");
    }
    if let Some(page) = content_stack.child_by_name(CONVERSION_PAGE) {
        content_stack.remove(&page);
    }
}

pub(super) fn wire_callbacks(
    view: &Rc<ConversionView>,
    staging: &StagingStore,
    deps: &ConversionWiring<'_>,
) {
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
        let player = deps.player.as_ref().map(Rc::downgrade);
        view.set_on_discard(move |job_id| {
            // FIX-6: if this render is the one currently previewing, stop the
            // preview first — otherwise the discard deletes the file out from
            // under the pipeline, leaving orphaned audio playing with no feedback.
            stop_preview_if_previewing(player.as_ref(), &staging, job_id);
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
        let player = deps.player.as_ref().map(Rc::downgrade);
        view.set_on_clear(move || {
            let has_undecided = view_weak
                .upgrade()
                .is_some_and(|view| view.has_undecided_now());
            if has_undecided {
                confirm_clear(&window, &conn, &staging, &view_weak, player.as_ref());
            } else {
                clear_undecided(&conn, &staging, &view_weak, player.as_ref());
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

/// Stops the live preview when `job_id`'s staging render is the one playing
/// (FIX-6), so discarding it never orphans audio from a file about to be
/// deleted. The correlation itself is the pure [`is_previewing_render`], so it
/// stays testable without a player.
fn stop_preview_if_previewing(
    player: Option<&std::rc::Weak<PlayerController>>,
    staging: &StagingStore,
    job_id: i64,
) {
    let Some(player) = player.and_then(std::rc::Weak::upgrade) else {
        return;
    };
    let render_path = staging.path_for_job(job_id);
    if is_previewing_render(player.previewing_path().as_deref(), render_path.to_str()) {
        player.stop_preview();
    }
}

/// Whether the live preview (`previewing`) is exactly `render` — the pure FIX-6
/// decision: a discard stops the preview only when it discards the very render
/// being previewed, never an unrelated one (and never when nothing is playing).
fn is_previewing_render(previewing: Option<&str>, render: Option<&str>) -> bool {
    matches!((previewing, render), (Some(p), Some(r)) if p == r)
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
pub(super) fn saved_job_count(conn: &Rc<RefCell<Connection>>) -> i64 {
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
    player: Option<&std::rc::Weak<PlayerController>>,
) {
    // FIX-6: clearing discards every undecided render, which necessarily
    // includes whichever one is being previewed — stop that preview first so no
    // audio keeps playing from a file about to be deleted.
    if let Some(player) = player.and_then(std::rc::Weak::upgrade) {
        player.stop_preview();
    }
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
    player: Option<&std::rc::Weak<PlayerController>>,
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
    let player = player.cloned();
    alert.connect_response(None, move |_, response| {
        if response == CLEAR_RESPONSE_DISCARD {
            clear_undecided(&conn, &staging, &view_weak, player.as_ref());
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
    use super::{
        install_conversions_page, is_previewing_render, play_target, remove_conversions_page,
        PlayTarget, CONVERSION_PAGE,
    };
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

    // FIX-6: discarding a render stops the live preview ONLY when it is the very
    // render being previewed — never an unrelated preview, and never when
    // nothing is playing. (The pure correlation the discard/clear wiring uses.)
    #[test]
    fn discard_stops_only_the_render_currently_previewing() {
        assert!(
            is_previewing_render(Some("/staging/7.flac"), Some("/staging/7.flac")),
            "discarding the render that is previewing stops the preview"
        );
        assert!(
            !is_previewing_render(Some("/staging/7.flac"), Some("/staging/9.flac")),
            "discarding a different render leaves an unrelated preview playing"
        );
        assert!(
            !is_previewing_render(None, Some("/staging/7.flac")),
            "nothing is previewing, so a discard stops nothing"
        );
        assert!(
            !is_previewing_render(Some("/staging/7.flac"), None),
            "a non-UTF-8 render path can't correlate to the preview"
        );
    }

    // UX INST-13: the conversions content page is installed under the SAME
    // experimental gate as the sidebar row, on demand when the row is selected.
    // Reproduces the reviewer's live sequence — switch starts off (no page),
    // toggle on, then the router's ensure-before-select installs a real page —
    // so the row never selects a missing page. Idempotent: no duplicate page.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn inst_13_toggle_on_installs_the_conversions_page_before_selection() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let conn = Connection::open_in_memory().unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        let conn = Rc::new(RefCell::new(conn));
        let staging = StagingStore::new(dir.path().join("staging"));

        let content_stack = gtk4::Stack::new();
        content_stack.add_named(&gtk4::Label::new(Some("library")), Some("library"));
        content_stack.set_visible_child_name("library");

        // Switch off at construction: no page (the same gate as the row).
        assert!(
            install_conversions_page(&content_stack, &conn, &staging).is_none(),
            "the page is not installed while the experimental switch is off"
        );
        assert!(
            content_stack.child_by_name(CONVERSION_PAGE).is_none(),
            "the conversions page is absent while experimental is off"
        );

        // Live toggle-on, then the router's ensure-before-select installs it.
        crate::ui::instrumental::set_experimental_enabled(&conn.borrow(), true).unwrap();
        assert!(
            install_conversions_page(&content_stack, &conn, &staging).is_some(),
            "toggling the switch on installs the page on demand (INST-13)"
        );
        assert!(
            content_stack.child_by_name(CONVERSION_PAGE).is_some(),
            "the row now selects a real page, never a missing one (INST-13)"
        );

        // The selection the router performs now lands on the real page.
        content_stack.set_visible_child_name(CONVERSION_PAGE);
        assert_eq!(
            content_stack.visible_child_name().as_deref(),
            Some(CONVERSION_PAGE),
            "selecting the Conversions row switches the content to its page"
        );

        // Idempotent: a second ensure adds no duplicate page.
        assert!(
            install_conversions_page(&content_stack, &conn, &staging).is_none(),
            "the page installer is idempotent once the page exists"
        );

        // Live toggle-off immediately leaves and removes the whole feature page.
        crate::ui::instrumental::set_experimental_enabled(&conn.borrow(), false).unwrap();
        remove_conversions_page(&content_stack);
        assert_eq!(
            content_stack.visible_child_name().as_deref(),
            Some("library"),
            "disabling while Conversions is visible routes back to Library"
        );
        assert!(
            content_stack.child_by_name(CONVERSION_PAGE).is_none(),
            "the conversions surface is absent immediately after live disable"
        );
    }
}
