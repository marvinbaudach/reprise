//! Runtime tests for the external-changes live refresh.
//!
//! The read + filter + coalesce step ([`read_and_plan`]) is exercised
//! synchronously and headlessly here — a real migrated database, no threads, no
//! main context — so it stays in the default workspace suite. The single
//! display test drives the full async path (notifier wake → channel → drain →
//! `Sidebar::refresh`) and therefore stays `#[ignore]`d, run one-per-process by
//! `scripts/check-display-tests.sh`.

use super::{read_and_plan, RefreshPlan};

use reprise_core::db;
use reprise_core::events::{read_since, writer_token};
use reprise_core::library::{playlists, settings};

fn migrated() -> rusqlite::Connection {
    db::open_migrated(None).unwrap()
}

fn newest_change_id(conn: &rusqlite::Connection) -> i64 {
    read_since(conn, 0, None)
        .unwrap()
        .last()
        .expect("a change was expected")
        .id
}

#[test]
fn read_and_plan_on_an_empty_log_keeps_the_cursor_and_plans_nothing() {
    let conn = migrated();

    let (cursor, plan) = read_and_plan(&conn, 0, Some(writer_token()));

    assert_eq!(cursor, 0);
    assert!(plan.is_empty());
}

#[test]
fn read_and_plan_surfaces_a_foreign_playlist_and_advances_the_cursor() {
    let conn = migrated();
    playlists::create(&conn, "From MCP").unwrap();
    let newest = newest_change_id(&conn);

    // `None` excludes nothing, so the write reads as foreign.
    let (cursor, plan) = read_and_plan(&conn, 0, None);

    assert_eq!(cursor, newest);
    assert_eq!(
        plan,
        RefreshPlan {
            sidebar: true,
            track_list: true,
            conversion: false,
        }
    );
}

#[test]
fn read_and_plan_suppresses_own_writes_but_still_advances_the_cursor() {
    let conn = migrated();
    playlists::create(&conn, "Made in-app").unwrap();
    let newest = newest_change_id(&conn);

    // The facade wrote with this process's token; excluding it silences the
    // plan — but the cursor still moves past the row so it is never re-scanned.
    let (cursor, plan) = read_and_plan(&conn, 0, Some(writer_token()));

    assert_eq!(cursor, newest);
    assert!(plan.is_empty());
}

#[test]
fn read_and_plan_ignores_a_foreign_settings_only_change() {
    let conn = migrated();
    settings::set_setting(&conn, "color_scheme", "dark").unwrap();
    let newest = newest_change_id(&conn);

    let (cursor, plan) = read_and_plan(&conn, 0, None);

    assert_eq!(cursor, newest);
    assert!(
        plan.is_empty(),
        "a foreign settings write must not reload views in v1"
    );
}

/// UX EXT-1a: a playlist created through a second database connection appears
/// in the running app's sidebar without a restart.
///
/// The whole live-refresh chain runs: the notifier observes the foreign
/// commit (via `PRAGMA data_version`), the wake reads and coalesces the change,
/// the drain applies it on the main thread, and `Sidebar::refresh` rebuilds the
/// rows. The runtime excludes no writer here (`None`): `writer_token` is
/// process-global, so an in-process second connection shares this process's
/// token — surfacing all writers is exactly what lets it stand in for a foreign
/// process, the same substitution the notifier's own tests rely on.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn ext_1a_external_playlist_appears_in_the_sidebar_without_restart() {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use gtk4::glib::MainContext;
    use gtk4::prelude::*;
    use libadwaita as adw;
    use reprise_core::view_source::ViewSource;

    use crate::ui::sidebar::{find_row, Sidebar};

    gtk4::init().unwrap();

    // A file-backed database so the second connection and the notifier's own
    // connection observe the same WAL — an in-memory database can be neither
    // shared across connections nor watched.
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("reprise.db");
    drop(db::open_migrated(Some(&db_path)).unwrap());

    let conn = Rc::new(RefCell::new(db::open(Some(&db_path)).unwrap()));
    let app = adw::Application::builder()
        .application_id("org.reprise.Reprise.ExternalChangesTest")
        .build();
    app.register(None::<&gtk4::gio::Cancellable>).unwrap();
    let window = adw::ApplicationWindow::new(&app);
    let sidebar = Rc::new(Sidebar::new(conn.clone(), &window, || 0));

    {
        let sidebar = Rc::downgrade(&sidebar);
        super::start(
            &db_path,
            None,
            Rc::new(move |plan: RefreshPlan| {
                if plan.sidebar {
                    if let Some(sidebar) = sidebar.upgrade() {
                        sidebar.refresh("external change (test)");
                    }
                }
            }),
        );
    }

    // A *second* connection creates the playlist — the foreign write.
    let playlist_id = {
        let second = db::open(Some(&db_path)).unwrap();
        playlists::create(&second, "Created by another process").unwrap()
    };
    assert!(
        find_row(&sidebar.shared, &ViewSource::Playlist(playlist_id)).is_none(),
        "precondition: the sidebar must not show the playlist before the refresh"
    );

    // Pump the main context until the drain applies the refresh. This host's
    // inotify budget is exhausted, so the notifier polls (P0 uses 8s windows) —
    // wait generously.
    let deadline = Instant::now() + Duration::from_secs(8);
    let context = MainContext::default();
    loop {
        while context.iteration(false) {}
        if find_row(&sidebar.shared, &ViewSource::Playlist(playlist_id)).is_some() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the externally created playlist did not appear in the sidebar within 8s"
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    assert!(find_row(&sidebar.shared, &ViewSource::Playlist(playlist_id)).is_some());
}
