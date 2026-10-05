use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::browser::BrowserPlace;
use reprise_core::library::session;
use reprise_core::playback::{AudioEffects, PlaybackBackend, PlaybackError, PlaybackState};
use reprise_core::queue::{QueueSnapshot, Repeat};
use reprise_core::up_next::{QueueItem, UpNextQueue};
use reprise_core::view_source::ViewSource;

use super::*;
use crate::ui::nav_history::{NavHistory, NavPlace};
use crate::ui::playback::player_controller::PlayerController;
use crate::ui::test_settle::{settle_until, settle_until_mapped, DISPLAY_TEST_TIMEOUT};
use crate::ui::track_list::TrackList;

const REMEMBERED_SEARCH: &str = "remember me";
const TEST_ARGV: [&str; 1] = ["reprise-termination-test"];

/// A backend that never plays: the tests only save a session.
struct InertPlayback;

impl PlaybackBackend for InertPlayback {
    fn play(&self, _: reprise_core::playback::PlaybackItem<'_>) -> Result<(), PlaybackError> {
        Ok(())
    }

    fn play_uri(&self, _: &str) -> Result<(), PlaybackError> {
        Ok(())
    }

    fn toggle_pause(&self) -> Result<PlaybackState, PlaybackError> {
        Ok(PlaybackState::Paused)
    }

    fn seek_to(&self, _: i64) -> Result<(), PlaybackError> {
        Ok(())
    }

    fn set_volume(&self, _: f64) {}

    fn set_audio_effects(&self, _: AudioEffects) -> Result<(), PlaybackError> {
        Ok(())
    }

    fn stop(&self) -> Result<(), PlaybackError> {
        Ok(())
    }

    fn set_next(&self, _: Option<reprise_core::playback::PlaybackItem<'_>>) {}

    fn set_transition(&self, _: reprise_core::library::settings::TrackTransition, _: u8) {}
}

/// A main window with a remembered browser place and a saver wired to it, but
/// no close handler: only the termination route can write the session.
struct Fixture {
    app: adw::Application,
    conn: Rc<reprise_core::db::Db>,
    window: adw::ApplicationWindow,
    saver: Rc<SessionSaver>,
    _track_list: Rc<TrackList>,
    _player: Option<Rc<PlayerController>>,
    _test_root: tempfile::TempDir,
}

fn fixture(with_player: bool) -> Fixture {
    gtk4::init().unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.marvinbaudach.Reprise.TerminationSaveTest")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let test_root = tempfile::tempdir().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let track_list = Rc::new(TrackList::new(
        conn.clone(),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        crate::ui::track_list::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    ));
    let mut place = BrowserPlace::from(ViewSource::Library);
    place.track_state_mut().unwrap().search = REMEMBERED_SEARCH.into();
    track_list.restore_browser_place(&place);
    let nav_history = Rc::new(NavHistory::default());
    nav_history.record_route(&NavPlace::browser(place));
    let player = with_player.then(|| player_with_queue(test_root.path(), &conn));
    let geometry = Rc::new(Cell::new((900, 600, false)));
    let window = adw::ApplicationWindow::builder()
        .application(&app)
        .default_width(900)
        .default_height(600)
        .content(track_list.widget())
        .build();
    let saver = Rc::new(SessionSaver::new(
        &conn,
        &track_list,
        player.as_ref(),
        &session::SessionState::default(),
        &geometry,
        &nav_history,
    ));
    Fixture {
        app,
        conn,
        window,
        saver,
        _track_list: track_list,
        _player: player,
        _test_root: test_root,
    }
}

/// A player holding track 7 in the queue and track 8 in Up Next.
fn player_with_queue(
    test_root: &std::path::Path,
    conn: &Rc<reprise_core::db::Db>,
) -> Rc<PlayerController> {
    crate::test_db::connection(conn)
        .execute_batch(
            "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (7, '/music/queued.flac', 'Queued', 'Artist', 120000, 0);
             INSERT INTO tracks (id, path, title, artist, duration_ms, added_at)
             VALUES (8, '/music/next.flac', 'Next', 'Artist', 120000, 0);",
        )
        .unwrap();
    let controller = crate::ui::playback::test_support::controller_with_db(
        test_root,
        conn.clone(),
        Box::new(InertPlayback),
    );
    controller.set_random_start_chooser_for_test(|_| Ok(Vec::new()));
    controller.restore_session_queue(
        QueueSnapshot {
            ids: vec![7],
            order: vec![0],
            position: Some(0),
            repeat: Repeat::All,
            shuffled: false,
        },
        up_next_of(8),
        None,
        None,
    );
    controller
}

fn up_next_of(id: i64) -> UpNextQueue {
    let mut queue = UpNextQueue::default();
    queue.prepend(&[QueueItem::Track(id)]);
    queue
}

/// Runs the application until something quits it, with `act` scheduled on the
/// main loop once it is up. The hold keeps the application alive by itself,
/// so `run` returning in time proves a `quit()` took effect, not merely that
/// the last window went away. Returns whether it did.
fn run_until_quit(app: &adw::Application, act: impl FnOnce() + 'static) -> bool {
    let _hold = app.hold();
    let act = RefCell::new(Some(act));
    let timed_out = Rc::new(Cell::new(false));
    let finished = Rc::new(Cell::new(false));
    let app_for_watchdog = app.clone();
    let (timed_out_by_watchdog, finished_by_watchdog) = (timed_out.clone(), finished.clone());
    app.connect_activate(move |_| {
        if let Some(act) = act.take() {
            glib::idle_add_local_once(act);
        }
        let app = app_for_watchdog.clone();
        let (timed_out, finished) = (timed_out_by_watchdog.clone(), finished_by_watchdog.clone());
        glib::timeout_add_local_once(DISPLAY_TEST_TIMEOUT, move || {
            if !finished.get() {
                timed_out.set(true);
                app.quit();
            }
        });
    });
    app.run_with_args(&TEST_ARGV);
    finished.set(true);
    !timed_out.get()
}

fn saved_search(conn: &reprise_core::db::Db) -> Option<String> {
    let mut place = session::load(conn).browser_place?;
    assert_eq!(place.view_source(), ViewSource::Library);
    place.track_state_mut().map(|state| state.search.clone())
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn start_5a_a_termination_request_saves_the_visible_place_and_closes_the_window() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let f = fixture(false);
    f.window.present();
    settle_until_mapped(f._track_list.widget());
    let (window, saver) = (f.window.clone(), f.saver.clone());

    let quit_in_time = run_until_quit(&f.app, move || handle(&window, &saver));

    assert!(quit_in_time, "the request quit the application");
    assert!(
        !f.window.is_visible(),
        "the window closed like a normal close"
    );
    assert_eq!(
        saved_search(&f.conn),
        Some(REMEMBERED_SEARCH.to_owned()),
        "only `handle` could have saved: this test connects no close handler"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn start_5a_a_termination_request_in_compact_mode_saves_without_a_presented_window() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let f = fixture(false);
    assert!(
        !f.window.is_realized(),
        "compact mode never presents the library window"
    );
    let (window, saver) = (f.window.clone(), f.saver.clone());

    let quit_in_time = run_until_quit(&f.app, move || handle(&window, &saver));

    assert!(quit_in_time, "the request quit the application");
    assert!(!f.window.is_visible());
    assert_eq!(saved_search(&f.conn), Some(REMEMBERED_SEARCH.to_owned()));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn start_5a_a_termination_request_saves_the_queue_and_up_next() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let f = fixture(true);
    f.window.present();
    settle_until_mapped(f._track_list.widget());
    let (window, saver) = (f.window.clone(), f.saver.clone());

    let quit_in_time = run_until_quit(&f.app, move || handle(&window, &saver));

    assert!(quit_in_time);
    let restored = session::load(&f.conn);
    assert_eq!(restored.queue.ids, vec![7]);
    assert_eq!(restored.up_next, up_next_of(8));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn start_5b_the_application_quits_even_when_the_window_refuses_to_close() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let f = fixture(false);
    f.window.connect_close_request(|_| glib::Propagation::Stop);
    f.window.present();
    settle_until_mapped(f._track_list.widget());
    let (window, saver) = (f.window.clone(), f.saver.clone());

    let quit_in_time = run_until_quit(&f.app, move || handle(&window, &saver));

    assert!(quit_in_time, "the quit backstop ended a vetoed close");
    assert!(
        f.window.is_visible(),
        "the veto held; only the quit ended the process"
    );
    assert_eq!(saved_search(&f.conn), Some(REMEMBERED_SEARCH.to_owned()));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn start_5b_a_request_without_a_window_still_quits_the_application() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let f = fixture(false);
    f.app.set_default();
    let saver = f.saver.clone();

    let quit_in_time = run_until_quit(&f.app, move || handle_request(None, &saver));

    assert!(
        quit_in_time,
        "a swallowed request would leave the process running"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn start_5c_a_repeat_right_after_the_first_request_does_not_end_the_process_before_the_save() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let f = fixture(false);
    f.window.present();
    settle_until_mapped(f._track_list.widget());
    let shared = Arc::new(Shared::default());
    let (sender, received) = async_channel::bounded(1);
    let (feed, source) = mpsc::channel();
    let ended = Arc::new(AtomicUsize::new(0));
    let ended_by_thread = ended.clone();
    let listener = termination_relay::spawn(
        move |deliver| source.into_iter().for_each(deliver),
        shared.clone(),
        sender,
        move |_| {
            ended_by_thread.fetch_add(1, Ordering::SeqCst);
        },
    )
    .unwrap();
    // A closing terminal's two SIGHUPs, or SIGTERM followed by SIGHUP.
    feed.send(libc::SIGTERM).unwrap();
    feed.send(libc::SIGHUP).unwrap();
    let (window, saver) = (f.window.downgrade(), f.saver.clone());

    let quit_in_time = run_until_quit(&f.app, move || {
        glib::spawn_future_local(serve(received, shared, window, saver));
    });
    drop(feed);
    listener.join().unwrap();

    assert!(quit_in_time);
    assert_eq!(ended.load(Ordering::SeqCst), 0, "the repeat ended nothing");
    assert_eq!(saved_search(&f.conn), Some(REMEMBERED_SEARCH.to_owned()));
    assert!(settle_until(DISPLAY_TEST_TIMEOUT, || !f
        .window
        .is_visible()));
}
