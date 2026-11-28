//! `NAV-2` / `BROWSE-4a` against the exact main-window composition: the
//! history, sidebar, content stack and `win.nav-back` are the production
//! wiring, so a loss that only exists in how they interact shows up here.

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::browser::BrowserPlace;
use reprise_core::db::Db;
use reprise_core::modules::{self, CONCERTS_MODULE, NEW_RELEASES_MODULE};
use reprise_core::view_source::ViewSource;

use super::window_layout_test_hook::WindowLayoutTestHandles;

const ARTIST: &str = "The Artist";

fn pause(milliseconds: u64) {
    let context = gtk4::glib::MainContext::default();
    context.block_on(gtk4::glib::timeout_future(
        std::time::Duration::from_millis(milliseconds),
    ));
    while context.iteration(false) {}
}

fn settle() {
    let context = gtk4::glib::MainContext::default();
    while context.iteration(false) {}
}

fn trace_to_test_output() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .try_init();
}

/// The Music root narrowed by a search, as a session can end on it.
fn narrowed_library_place() -> BrowserPlace {
    BrowserPlace::tracks(
        reprise_core::browser::TrackCollection::Library(reprise_core::browser::LibraryScope::All),
        reprise_core::browser::TrackViewState {
            search: "Track".to_owned(),
            ..Default::default()
        },
    )
}

/// Builds the production window over a database that has the Releases and
/// Concerts modules on, an artist with tracks, and `startup_place` as the
/// place the previous session ended on.
fn build_window(startup_place: Option<BrowserPlace>) -> WindowLayoutTestHandles {
    build_window_with_root(startup_place.clone(), startup_place)
}

fn build_window_with_root(
    startup_place: Option<BrowserPlace>,
    library_root: Option<BrowserPlace>,
) -> WindowLayoutTestHandles {
    gtk4::init().expect("GTK test display");
    trace_to_test_output();
    let app = adw::Application::builder()
        .application_id("de.reprise.Reprise.RealWindowNavTest")
        .build();
    app.register(None::<&gtk4::gio::Cancellable>)
        .expect("register test application");
    let db = std::rc::Rc::new(crate::test_db::open().expect("open test database"));
    seed_artist_tracks(&db);
    modules::set_enabled(&db, &NEW_RELEASES_MODULE, true).expect("enable Releases");
    modules::set_enabled(&db, &CONCERTS_MODULE, true).expect("enable Concerts");
    let state = reprise_core::library::session::SessionState {
        window_width: 800,
        window_height: 600,
        browser_place: startup_place,
        library_root,
        queue: played_queue(&db),
        play_origin: Some(reprise_core::library::session::SessionSource::Library),
        ..Default::default()
    };
    reprise_core::library::session::save(&db, &state).expect("save the previous session");
    let db_path = db.path().expect("file-backed test database").to_path_buf();
    let _handler = super::surface::build(
        &app,
        &db,
        &db_path,
        crate::ui::file_open::StartupOpenIntent::Library,
    );
    let handles = super::window_layout_test_hook::take()
        .expect("window composition publishes layout test handles");
    pause(300);
    handles
}

/// A restored player session whose current track sits outside the narrowed
/// Music view, as a previous run that ended mid-listen leaves it.
fn played_queue(db: &Db) -> reprise_core::queue::QueueSnapshot {
    let connection = crate::test_db::connection(db);
    let mut statement = connection
        .prepare("SELECT id FROM tracks ORDER BY id")
        .expect("prepare the track id query");
    let ids: Vec<i64> = statement
        .query_map([], |row| row.get(0))
        .expect("query track ids")
        .filter_map(Result::ok)
        .collect();
    reprise_core::queue::QueueSnapshot {
        order: (0..ids.len()).collect(),
        position: ids.len().checked_sub(1),
        ids,
        repeat: reprise_core::queue::Repeat::default(),
        shuffled: false,
    }
}

fn seed_artist_tracks(db: &Db) {
    let connection = crate::test_db::connection(db);
    for index in 0..5 {
        connection
            .execute(
                "INSERT INTO tracks (path, title, artist, album_artist, album, year, duration_ms, added_at) \
                 VALUES (?1, ?2, ?3, ?3, 'The Album', 2025, 754000, 0)",
                rusqlite::params![
                    format!("/test/real-window-nav-{index}.flac"),
                    format!("Track {index}"),
                    ARTIST
                ],
            )
            .expect("seed a track of the artist");
    }
    for index in 0..3 {
        connection
            .execute(
                "INSERT INTO tracks (path, title, artist, album_artist, album, year, duration_ms, added_at) \
                 VALUES (?1, ?2, 'Someone Else', 'Someone Else', 'Other Album', 2025, 754000, 0)",
                rusqlite::params![
                    format!("/test/real-window-nav-other-{index}.flac"),
                    format!("Other {index}")
                ],
            )
            .expect("seed a track of another artist");
    }
    connection
        .execute(
            "INSERT INTO new_releases (
               release_group_mbid, artist_name, artist_mbid, title, release_type,
               first_release_date, fetched_at
             ) VALUES ('release-1', ?1, 'artist-id', 'New Album', 'Album', '2026-08-05', 1)",
            rusqlite::params![ARTIST],
        )
        .expect("seed a release of the artist");
}

fn descendant_column_view(root: &gtk4::Widget) -> Option<gtk4::ColumnView> {
    if let Ok(column_view) = root.clone().downcast::<gtk4::ColumnView>() {
        return Some(column_view);
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(found) = descendant_column_view(&widget) {
            return Some(found);
        }
        child = widget.next_sibling();
    }
    None
}

/// The Releases page's own "Go to artist" menu action on its first row,
/// reached through the action group the real view inserts on its table.
fn releases_go_to_artist(handles: &WindowLayoutTestHandles) {
    let page = handles
        .content_stack
        .child_by_name("releases")
        .expect("the window has a Releases page");
    let table = descendant_column_view(&page).expect("the Releases page has a table");
    let selection = table
        .model()
        .expect("the Releases table has a selection model");
    assert!(selection.n_items() > 0, "the seeded release is listed");
    selection.select_item(0, true);
    table
        .activate_action("releases.go-to-artist", None)
        .expect("the Releases menu action exists");
    settle();
}

fn visible_page(handles: &WindowLayoutTestHandles) -> Option<String> {
    handles
        .content_stack
        .visible_child_name()
        .map(|name| name.to_string())
}

/// Clicks a sidebar row exactly as the user does: selecting the list row is
/// what runs the production `on_select` routing.
fn click_row(handles: &WindowLayoutTestHandles, source: &ViewSource) {
    let row = crate::ui::sidebar::find_row(&handles.sidebar.shared, source)
        .unwrap_or_else(|| panic!("the sidebar has no row for {}", source.label()));
    handles.sidebar.shared.listbox.select_row(Some(&row));
    settle();
}

/// The Releases context menu's "Go to artist" and the track list's artist
/// link both end in the window's one `MetadataNavigator`.
fn go_to_artist(handles: &WindowLayoutTestHandles) {
    let callback = handles
        .track_list
        .shared
        .on_go_to_artist
        .borrow()
        .clone()
        .expect("the window wires the Go to artist link");
    callback(0, ARTIST.to_owned());
    settle();
}

fn descendant_back_button(root: &gtk4::Widget) -> Option<gtk4::Button> {
    if let Ok(button) = root.clone().downcast::<gtk4::Button>() {
        if button.action_name().as_deref() == Some("win.nav-back") {
            return Some(button);
        }
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(found) = descendant_back_button(&widget) {
            return Some(found);
        }
        child = widget.next_sibling();
    }
    None
}

/// Clicks the header's Back button, the control the user presses.
fn nav_back(handles: &WindowLayoutTestHandles) {
    let button = descendant_back_button(handles.window.upcast_ref())
        .expect("the header carries a Back button");
    assert!(button.is_sensitive(), "Back is enabled after a navigation");
    button.emit_clicked();
    settle();
}

fn assert_artist_opened(handles: &WindowLayoutTestHandles) {
    assert_eq!(visible_page(handles).as_deref(), Some("library"));
    assert!(matches!(
        handles.track_list.current_source(),
        ViewSource::Artist(_)
    ));
}

fn assert_back_from_artist_returns_to(
    handles: &WindowLayoutTestHandles,
    source: &ViewSource,
    open_artist: fn(&WindowLayoutTestHandles),
) {
    let page = if *source == ViewSource::Releases {
        "releases"
    } else {
        "concerts"
    };
    assert_eq!(visible_page(handles).as_deref(), Some(page), "section open");
    open_artist(handles);
    pause(500);
    assert_artist_opened(handles);

    nav_back(handles);

    assert_eq!(
        visible_page(handles).as_deref(),
        Some(page),
        "Back from the artist list must return to the section it was opened from"
    );
    assert_eq!(*handles.sidebar.shared.current_source.borrow(), *source);
    handles.window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn browse_4a_back_from_go_to_artist_returns_to_releases() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let handles = build_window(None);
    click_row(&handles, &ViewSource::Releases);
    pause(500);
    assert_back_from_artist_returns_to(&handles, &ViewSource::Releases, releases_go_to_artist);
}

/// An update notification opens the view through the sidebar's programmatic
/// selection, not through a row click.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn browse_4a_back_from_go_to_artist_returns_to_releases_opened_by_a_notification() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let handles = build_window(None);
    handles
        .sidebar
        .refresh_and_select(ViewSource::Releases, "update notification activated");
    settle();
    pause(500);
    assert_back_from_artist_returns_to(&handles, &ViewSource::Releases, releases_go_to_artist);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn browse_4a_back_from_go_to_artist_returns_to_releases_after_a_restart_on_a_narrowed_music() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let handles = build_window(Some(narrowed_library_place()));
    click_row(&handles, &ViewSource::Releases);
    pause(500);
    assert_back_from_artist_returns_to(&handles, &ViewSource::Releases, releases_go_to_artist);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn browse_4a_back_from_go_to_artist_returns_to_releases_after_a_restart_there() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let handles = build_window(Some(BrowserPlace::Releases));
    assert_back_from_artist_returns_to(&handles, &ViewSource::Releases, releases_go_to_artist);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn browse_4a_back_from_go_to_artist_returns_to_concerts() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let handles = build_window(None);
    click_row(&handles, &ViewSource::Concerts);
    pause(500);
    assert_back_from_artist_returns_to(&handles, &ViewSource::Concerts, go_to_artist);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_2_back_after_a_sidebar_click_from_releases_returns_to_releases() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let handles = build_window(None);
    click_row(&handles, &ViewSource::Releases);
    click_row(&handles, &ViewSource::Library);
    assert_eq!(visible_page(&handles).as_deref(), Some("library"));

    nav_back(&handles);

    assert_eq!(visible_page(&handles).as_deref(), Some("releases"));
    handles.window.close();
}

fn descendant_place_pill(root: &gtk4::Widget) -> Option<gtk4::Button> {
    if let Ok(button) = root.clone().downcast::<gtk4::Button>() {
        if button.has_css_class(crate::ui::browse::browse_bar::PLACE_PILL_CSS_CLASS)
            && button.is_mapped()
        {
            return Some(button);
        }
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(found) = descendant_place_pill(&widget) {
            return Some(found);
        }
        child = widget.next_sibling();
    }
    None
}

/// Clicks the filter row's "‹ <place>" pill, which reads as a back step.
fn leave_place(handles: &WindowLayoutTestHandles) {
    let pill = descendant_place_pill(handles.window.upcast_ref())
        .expect("the artist list shows its place pill");
    pill.emit_clicked();
    settle();
    pause(300);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fil_1e_the_place_pill_returns_to_releases_after_go_to_artist() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let handles = build_window(None);
    click_row(&handles, &ViewSource::Releases);
    pause(500);
    releases_go_to_artist(&handles);
    pause(500);
    assert_artist_opened(&handles);

    leave_place(&handles);

    assert_eq!(
        visible_page(&handles).as_deref(),
        Some("releases"),
        "leaving the artist place steps back to the section it was entered from"
    );
    assert_eq!(
        *handles.sidebar.shared.current_source.borrow(),
        ViewSource::Releases
    );
    handles.window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fil_1e_the_place_pill_without_history_leaves_for_the_library() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let artist_place = BrowserPlace::tracks(
        reprise_core::browser::TrackCollection::Library(
            reprise_core::browser::LibraryScope::Artist(reprise_core::browser::ArtistKey::new(
                ARTIST,
            )),
        ),
        reprise_core::browser::TrackViewState::default(),
    );
    let handles = build_window_with_root(Some(artist_place), None);
    assert_artist_opened(&handles);

    leave_place(&handles);

    assert_eq!(visible_page(&handles).as_deref(), Some("library"));
    assert_eq!(handles.track_list.current_source(), ViewSource::Library);
    handles.window.close();
}
