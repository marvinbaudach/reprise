//! Display and interaction tests for the Albums view.

use std::cell::Cell;
use std::time::{Duration, Instant};

use super::*;

const NAVIGATION_RESTORE_TIMEOUT_MS: u64 = 3_000;

fn wait_for_layout(milliseconds: u64) {
    let main_loop = gtk4::glib::MainLoop::new(None, false);
    let quit = main_loop.clone();
    gtk4::glib::timeout_add_local_once(Duration::from_millis(milliseconds), move || {
        quit.quit();
    });
    main_loop.run();
}

fn wait_until(milliseconds: u64, mut predicate: impl FnMut() -> bool + 'static) -> bool {
    let main_loop = gtk4::glib::MainLoop::new(None, false);
    let quit = main_loop.clone();
    let matched = Rc::new(Cell::new(false));
    let matched_for_tick = matched.clone();
    let deadline = Instant::now() + Duration::from_millis(milliseconds);
    gtk4::glib::timeout_add_local(Duration::from_millis(5), move || {
        if predicate() {
            matched_for_tick.set(true);
            quit.quit();
            gtk4::glib::ControlFlow::Break
        } else if Instant::now() >= deadline {
            quit.quit();
            gtk4::glib::ControlFlow::Break
        } else {
            gtk4::glib::ControlFlow::Continue
        }
    });
    main_loop.run();
    matched.get()
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn grid_8_album_view_fills_the_available_library_height() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    for index in 0..12 {
        conn.execute(
            "INSERT INTO tracks (path,title,artist,album,added_at) VALUES (?1,?2,'Artist',?3,?4)",
            rusqlite::params![
                format!("/grid-height-{index}.flac"),
                format!("Track {index}"),
                format!("Album {index:02}"),
                index,
            ],
        )
        .unwrap();
    }
    let conn = Rc::new(RefCell::new(conn));
    let loader = crate::ui::cover_loader::CoverLoader::new(
        crate::ui::cover_download_worker::setup_for_test(),
    );
    let view = AlbumView::new(&conn, loader);

    let tracks = gtk4::Label::new(Some("Tracks"));
    tracks.set_vexpand(true);
    let library = libadwaita::ViewStack::builder()
        .hhomogeneous(false)
        .transition_duration(crate::ui::motion::STANDARD_MS)
        .build();
    library.add_named(&tracks, Some("tracks"));
    library.add_named(view.widget(), Some("albums"));
    library.set_visible_child_name("tracks");

    let scrolled = view
        .grid_widget()
        .parent()
        .and_downcast::<gtk4::ScrolledWindow>()
        .expect("the album grid must remain the native scroller child");
    let page_stack = scrolled
        .parent()
        .and_downcast::<gtk4::Stack>()
        .expect("the album scroller must remain on the grid page");
    let content = page_stack
        .parent()
        .and_downcast::<gtk4::Box>()
        .expect("the album grid page must remain in its content column");
    let ambient = content
        .parent()
        .and_downcast::<gtk4::Overlay>()
        .expect("the album content column must remain above the ambient glow");

    let window = gtk4::Window::builder()
        .default_width(800)
        .default_height(600)
        .child(&library)
        .build();
    window.present();
    wait_for_layout(50);
    library.set_visible_child_name("albums");
    wait_for_layout(u64::from(crate::ui::motion::STANDARD_MS + 50));

    let geometry = format!(
        "library={} album={} ambient={} content={} page_stack={} scroller={} grid={}",
        library.height(),
        view.widget().height(),
        ambient.height(),
        content.height(),
        page_stack.height(),
        scrolled.height(),
        view.grid_widget().height(),
    );
    assert_eq!(
        view.widget().height(),
        library.height(),
        "the album root must fill the Library viewport: {geometry}"
    );
    assert_eq!(
        ambient.height(),
        view.widget().height(),
        "the ambient Album layer collapsed to the card rows: {geometry}"
    );
    assert_eq!(
        content.height(),
        ambient.height(),
        "the Album content did not fill its ambient layer: {geometry}"
    );
    assert!(
        page_stack.height() * 2 > library.height(),
        "the Album grid page remained clipped near its natural height: {geometry}"
    );
    assert_eq!(
        scrolled.height(),
        page_stack.height(),
        "the Album scroller lost the grid-page allocation: {geometry}"
    );
    window.close();
}

fn descendants_with_class(root: &gtk4::Widget, class: &str) -> Vec<gtk4::Widget> {
    let mut matches = Vec::new();
    let mut pending = vec![root.clone()];
    while let Some(widget) = pending.pop() {
        if widget.has_css_class(class) {
            matches.push(widget.clone());
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            pending.push(current.clone());
            child = current.next_sibling();
        }
    }
    matches
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_5_remembers_scroll_and_selection_per_view() {
    gtk4::init().unwrap();
    let conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    for index in 0..36 {
        conn.execute(
            "INSERT INTO tracks (path,title,artist,album,added_at) VALUES (?1,?2,'Artist',?3,0)",
            rusqlite::params![
                format!("/nav5-{index}.flac"),
                format!("Track {index}"),
                format!("Album {index:02}"),
            ],
        )
        .unwrap();
    }
    let conn = Rc::new(RefCell::new(conn));
    let loader = crate::ui::cover_loader::CoverLoader::new(
        crate::ui::cover_download_worker::setup_for_test(),
    );
    let view = AlbumView::new(&conn, loader);
    let window = gtk4::Window::builder()
        .default_width(500)
        .default_height(480)
        .child(view.widget())
        .build();
    window.present();
    wait_for_layout(100);

    view.selection.set_selected(20);
    let adjustment = view.grid_widget().vadjustment().unwrap();
    adjustment.set_value((adjustment.upper() - adjustment.page_size()) * 0.6);
    let remembered = adjustment.value();
    view.remember_view_state_callback()();
    view.selection.unselect_all();
    adjustment.set_value(0.0);
    view.restore_view_state_callback()();

    let selection = view.selection.clone();
    let adjustment_for_wait = adjustment.clone();
    assert!(wait_until(NAVIGATION_RESTORE_TIMEOUT_MS, move || {
        selection.selected() == 20 && (adjustment_for_wait.value() - remembered).abs() < 2.0
    }));
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn keyboard_activate_on_grid_opens_album() {
    gtk4::init().unwrap();
    let conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    conn.execute_batch(
        "INSERT INTO tracks (path,title,artist,album,added_at) VALUES
         ('/one.flac','One','Artist A','First',0);",
    )
    .unwrap();
    let conn = Rc::new(RefCell::new(conn));
    let loader = crate::ui::cover_loader::CoverLoader::new(
        crate::ui::cover_download_worker::setup_for_test(),
    );
    let view = AlbumView::new(&conn, loader);
    assert!(view
        .grid_widget()
        .model()
        .is_some_and(|model| model.is::<gtk4::SingleSelection>()));

    let activated: Rc<RefCell<Option<AlbumSummary>>> = Rc::new(RefCell::new(None));
    {
        let activated = activated.clone();
        view.set_on_activate(move |album| {
            *activated.borrow_mut() = Some(album);
        });
    }

    // `activate` is the signal GridView's built-in Enter binding emits
    // for the focused cell — emitting it directly exercises the same
    // handler the keyboard path runs.
    view.grid_widget().emit_by_name::<()>("activate", &[&0u32]);

    let opened = activated.borrow();
    assert_eq!(opened.as_ref().map(|a| a.album.as_str()), Some("First"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn album_grid_loads_from_query_and_supports_filter() {
    gtk4::init().unwrap();
    let conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    conn.execute_batch(
        "INSERT INTO tracks (path,title,artist,album,added_at) VALUES
         ('/one.flac','One','Artist A','First',0),
         ('/two.flac','Two','Artist B','Second',0);",
    )
    .unwrap();
    let conn = Rc::new(RefCell::new(conn));
    let loader = crate::ui::cover_loader::CoverLoader::new(
        crate::ui::cover_download_worker::setup_for_test(),
    );
    let view = AlbumView::new(&conn, loader);

    assert_eq!(view.album_count(), 2);

    let filter = view.filter_callback();
    filter("first");
    assert_eq!(view.state.filtered_count(), 1);

    filter("");
    assert_eq!(view.state.filtered_count(), 2);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn grid_7_album_view_uses_one_static_preblurred_now_playing_texture() {
    gtk4::init().unwrap();
    let conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    let conn = Rc::new(RefCell::new(conn));
    let loader = crate::ui::cover_loader::CoverLoader::new(
        crate::ui::cover_download_worker::setup_for_test(),
    );
    let view = AlbumView::new(&conn, loader);

    assert!(view.glow.picture().is_ancestor(view.widget()));
    assert!(view.glow.picture().has_css_class("album-now-playing-glow"));
    assert!(!view.glow.picture().can_target());
    view.now_playing_callback()(Some(crate::ui::current_track_selection::NowPlayingAlbum {
        album: "Album".into(),
        artist: "Artist".into(),
        track_path: "/missing-track.flac".into(),
    }));
    assert_eq!(view.glow.generation(), 1);
    assert!(!view.glow.picture().is_visible());
    view.now_playing_callback()(None);
    assert_eq!(view.glow.generation(), 2);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn grid_5_reveal_scrolls_to_playing_album() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let settings = gtk4::Settings::default().unwrap();
    let animations_before = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(false);

    let conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    for index in 0..30 {
        conn.execute(
            "INSERT INTO tracks (path,title,artist,album,added_at) \
             VALUES (?1,?2,'Artist',?3,0)",
            rusqlite::params![
                format!("/album-{index}.flac"),
                format!("Track {index}"),
                format!("Album {index:02}"),
            ],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO tracks (path,title,artist,album,added_at) \
         VALUES ('/playing.flac','Playing track','Artist B','ZZ Playing',0)",
        [],
    )
    .unwrap();
    let conn = Rc::new(RefCell::new(conn));
    let loader = crate::ui::cover_loader::CoverLoader::new(
        crate::ui::cover_download_worker::setup_for_test(),
    );
    let view = AlbumView::new(&conn, loader);
    let player_surface = gtk4::Button::with_label("Player title");
    let shell = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    shell.append(&player_surface);
    view.widget().set_vexpand(true);
    shell.append(view.widget());
    let window = gtk4::Window::builder()
        .default_width(500)
        .default_height(600)
        .child(&shell)
        .build();
    window.present();
    wait_for_layout(50);
    assert!(
        player_surface.grab_focus(),
        "fixture starts from a focused player surface"
    );

    view.now_playing_callback()(Some(crate::ui::current_track_selection::NowPlayingAlbum {
        album: "ZZ Playing".into(),
        artist: "Artist B".into(),
        track_path: "/playing.flac".into(),
    }));
    view.filter_callback()("no visible album");
    assert_eq!(view.state.filtered_count(), 0);

    let adjustment = view.grid_widget().vadjustment().unwrap();
    assert_eq!(adjustment.value(), 0.0);
    assert!(view.reveal_callback()("ZZ Playing", "Artist B"));
    let grid_for_wait = view.grid_widget().clone();
    let adjustment_for_wait = adjustment.clone();
    // Wait for BOTH conditions in one predicate. Sequential waits make the
    // outcome depend on which half completes first, which silently
    // inverted this diagnostic once the implementation began focusing
    // before scrolling. Each condition is reported separately on timeout.
    let settled = wait_until(1000, {
        let adjustment = adjustment_for_wait.clone();
        let grid = grid_for_wait.clone();
        move || adjustment.value() > 0.0 && grid.focus_child().is_some()
    });
    assert!(
        settled,
        "reveal must scroll and focus; scrolled={} focused={} \
         (adjustment value={} upper={} page={})",
        adjustment.value() > 0.0,
        view.grid_widget().focus_child().is_some(),
        adjustment.value(),
        adjustment.upper(),
        adjustment.page_size()
    );
    assert_eq!(
        view.state.filtered_count(),
        31,
        "reveal clears the album filter"
    );
    assert!(
        adjustment.value() > 0.0,
        "GtkGridView scrolled through its vertical adjustment"
    );

    let focused = view
        .grid_widget()
        .focus_child()
        .expect("GtkGridView focused the revealed item");
    let title = descendants_with_class(&focused, crate::ui::album_card_css::TITLE_CLASS)
        .into_iter()
        .next()
        .and_downcast::<gtk4::Label>()
        .expect("revealed title");
    assert_eq!(title.text(), "ZZ Playing");

    let reveal_frame =
        descendants_with_class(&focused, crate::ui::album_card_css::REVEAL_FRAME_CLASS)
            .into_iter()
            .next()
            .expect("cover-only reveal frame");
    assert!(reveal_frame.has_css_class(crate::ui::album_card_css::REVEAL_PULSE_STATIC_CLASS));
    let playing_layer =
        descendants_with_class(&focused, crate::ui::album_card_css::PLAYING_LAYER_CLASS)
            .into_iter()
            .next()
            .expect("persistent playing layer");
    assert!(playing_layer.is_visible());

    wait_for_layout(crate::ui::album_card_css::REVEAL_DURATION_MS + 50);
    assert!(!reveal_frame.has_css_class(crate::ui::album_card_css::REVEAL_PULSE_STATIC_CLASS));
    assert_eq!(view.grid_widget().focus_child(), Some(focused));
    assert!(playing_layer.is_visible());

    window.close();
    settings.set_gtk_enable_animations(animations_before);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn grid_6_restore_focus_targets_departed_album_without_reveal_pulse() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    conn.execute_batch(
        "INSERT INTO tracks (path,title,artist,album,added_at) VALUES
         ('/one.flac','One','Artist A','First',0),
         ('/two.flac','Two','Artist B','Return Here',0),
         ('/three.flac','Three','Artist C','Last',0);",
    )
    .unwrap();
    let conn = Rc::new(RefCell::new(conn));
    let loader = crate::ui::cover_loader::CoverLoader::new(
        crate::ui::cover_download_worker::setup_for_test(),
    );
    let view = AlbumView::new(&conn, loader);
    let window = gtk4::Window::builder()
        .default_width(500)
        .default_height(600)
        .child(view.widget())
        .build();
    window.present();
    wait_for_layout(50);

    assert!(view.restore_focus_callback()("Return Here", "Artist B"));
    wait_for_layout(50);

    let focused = view
        .grid_widget()
        .focus_child()
        .expect("GtkGridView focused the restored album");
    let title = descendants_with_class(&focused, crate::ui::album_card_css::TITLE_CLASS)
        .into_iter()
        .next()
        .and_downcast::<gtk4::Label>()
        .expect("restored title");
    assert_eq!(title.text(), "Return Here");
    let reveal_frame =
        descendants_with_class(&focused, crate::ui::album_card_css::REVEAL_FRAME_CLASS)
            .into_iter()
            .next()
            .expect("cover-only reveal frame");
    assert!(!reveal_frame.has_css_class(crate::ui::album_card_css::REVEAL_PULSE_CLASS));
    assert!(!reveal_frame.has_css_class(crate::ui::album_card_css::REVEAL_PULSE_STATIC_CLASS));

    window.close();
}
