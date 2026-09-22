//! FB-14 coverage for the Updates popover's two `show_loading` call sites:
//! the open-triggered path in `wire()` and the fetch-start path in
//! `popover_fetch::start_fetch`. Both must keep already-cached rows on
//! screen for the whole fetch and fall back to the loading row only when the
//! popover has nothing cached to show — see `popover.rs`'s
//! `has_cached_content`.

use super::popover_fetch::FetchTrigger;
use super::tests::{noop_show_album, test_popover};
use super::*;

/// Seeds one recent release, close enough to today to survive
/// `delta_candidates`'s 90-day window without pinning a fixed date.
fn seed_recent_release(conn: &reprise_core::db::Db) {
    let today = chrono::Local::now().date_naive();
    let recent = (today - chrono::Duration::days(10))
        .format("%Y-%m-%d")
        .to_string();
    crate::test_db::connection(conn)
        .execute(
            "INSERT INTO new_releases (
               release_group_mbid, artist_name, artist_mbid, title, release_type,
               first_release_date, fetched_at
             ) VALUES ('release', 'Artist', 'artist', 'Release', 'Album', ?1, 1)",
            rusqlite::params![recent],
        )
        .unwrap();
}

/// A view that already holds cached rows keeps them on screen while a fetch
/// runs — mirroring `a_refresh_keeps_the_cached_rows_on_screen` for the
/// podcast list (#1005). The open path follows `popup()` immediately, so
/// blanking the popover here would hide a fully loaded snapshot for the
/// whole network round trip.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn a_fetch_in_flight_keeps_the_cached_content_on_screen() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::NEW_RELEASES_MODULE, true)
        .unwrap();
    reprise_core::library::settings::set_new_releases_fetch_completed(&conn, true).unwrap();
    seed_recent_release(&conn);
    let conn = Rc::new(conn);
    let state = test_popover(conn, PathBuf::from("unused.db"));

    assert!(
        state.has_cached_content(),
        "the fixture must seed a release the snapshot actually finds, or this \
         test cannot tell a broken fixture from a broken guard"
    );

    let window = gtk4::Window::new();
    window.set_child(Some(&state.button));
    window.present();
    // Simulates the open-triggered background refresh without spawning the
    // real worker: `periodic_fetch_due` treats a fetch already in flight as
    // never due, so `maybe_background_refresh` inside `connect_show` is a
    // no-op here.
    state.fetching.set(true);
    state.popover.popup();
    while gtk4::glib::MainContext::default().iteration(false) {}

    assert_eq!(
        state.content_stack.visible_child_name().as_deref(),
        Some("content"),
        "cached rows stay on screen while the open-triggered fetch runs"
    );
    state.popover.popdown();
    window.close();
}

/// A view without anything cached has nothing to keep: the loading row owns
/// the stack while the fetch runs, rather than an empty state the fetch may
/// contradict.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn an_empty_snapshot_with_a_fetch_in_flight_shows_the_loading_row() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::NEW_RELEASES_MODULE, true)
        .unwrap();
    let conn = Rc::new(conn);
    let state = test_popover(conn, PathBuf::from("unused.db"));

    assert!(
        !state.has_cached_content(),
        "an empty database must not report cached content"
    );

    let window = gtk4::Window::new();
    window.set_child(Some(&state.button));
    window.present();
    state.fetching.set(true);
    state.popover.popup();
    while gtk4::glib::MainContext::default().iteration(false) {}

    assert_eq!(
        state.content_stack.visible_child_name().as_deref(),
        Some("loading")
    );
    state.popover.popdown();
    window.close();
}

/// The footer's own "reload" button reaches `start_fetch` directly, without
/// ever going through `wire()`'s open path. Its own `show_loading` call
/// needs the same guard, or a manual reload blanks an already-loaded
/// popover for the whole fetch.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn a_manual_reload_keeps_the_cached_content_on_screen() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::NEW_RELEASES_MODULE, true)
        .unwrap();
    reprise_core::library::settings::set_new_releases_fetch_completed(&conn, true).unwrap();
    seed_recent_release(&conn);
    let conn = Rc::new(conn);
    let concerts_runtime = ConcertsRuntime::setup(&conn);
    let state = NewReleasesPopover::new(
        conn,
        PathBuf::from("unused.db"),
        concerts_runtime,
        noop_show_album(),
        Rc::new(|_| {}),
    );
    assert!(state.has_cached_content());
    assert_eq!(
        state.content_stack.visible_child_name().as_deref(),
        Some("content")
    );

    state.start_fetch(FetchTrigger::Manual);

    assert_eq!(
        state.content_stack.visible_child_name().as_deref(),
        Some("content"),
        "cached rows stay on screen while a manually triggered reload runs"
    );
    assert!(state.fetching.get(), "the reload must reach the worker");
}
