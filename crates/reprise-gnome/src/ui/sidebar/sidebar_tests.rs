use super::*;

/// Builds a bare `Shared` over a fresh in-memory database — enough for
/// `rebuild` and the drop-handler functions, without an `adw::
/// ApplicationWindow`/`Application` (the `window`/`toast_overlay` weak refs
/// simply stay unset, which every consumer already degrades on). Display
/// required (`gtk::ListBox`), hence only the `#[ignore]` tests below use it.
fn test_shared() -> Rc<Shared> {
    let conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    Rc::new(Shared {
        conn: Rc::new(RefCell::new(conn)),
        listbox: gtk4::ListBox::new(),
        issues_listbox: gtk4::ListBox::new(),
        queue_len_provider: Box::new(|| 0),
        current_source: RefCell::new(ViewSource::default()),
        rows: RefCell::new(Vec::new()),
        new_playlist_row: RefCell::new(None),
        import_playlist_row: RefCell::new(None),
        on_select: RefCell::new(None),
        on_show_content: RefCell::new(None),
        on_import_playlist: RefCell::new(None),
        on_tracks_added: RefCell::new(None),
        on_remove_missing: RefCell::new(None),
        on_queue_drop: RefCell::new(None),
        window: glib::WeakRef::new(),
        toast_overlay: glib::WeakRef::new(),
        refresh_count: Cell::new(0),
    })
}

/// Recursively finds the first descendant `Label` carrying the `numeric`
/// CSS class — the sidebar's trailing count-badge label (see
/// `sidebar_presentation::build_nav_row`).
fn numeric_badge_text(widget: &gtk4::Widget) -> Option<String> {
    if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
        if label.has_css_class("numeric") {
            return Some(label.text().to_string());
        }
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(found) = numeric_badge_text(&current) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

fn has_drop_target(row: &gtk4::ListBoxRow) -> bool {
    let controllers = row.observe_controllers();
    (0..controllers.n_items()).any(|index| {
        controllers
            .item(index)
            .is_some_and(|controller| controller.is::<gtk4::DropTarget>())
    })
}

/// Waits until the toplevel holds the global input focus.
///
/// `gtk_widget_has_focus()` is `is_focus() && window.is_active()`, and X
/// delivers the activation asynchronously — measured at ~21 ms under Xvfb.
/// A non-blocking drain returns long before that, so any test that exercises a
/// `has_focus()`-gated code path must wait here first. `iteration(true)` blocks
/// until there is something to dispatch rather than spinning a core.
fn settle_until_active(window: &gtk4::Window) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while !window.is_active() {
        assert!(
            std::time::Instant::now() < deadline,
            "test window did not become active within 2s; \
             the display server must grant focus for has_focus() assertions"
        );
        gtk4::glib::MainContext::default().iteration(true);
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn queue_row_installs_a_drop_target_but_library_row_does_not() {
    gtk4::init().unwrap();
    let shared = test_shared();
    rebuild(&shared, None, "test build");

    let queue_row = find_row(&shared, &ViewSource::Queue).unwrap();
    assert!(
        has_drop_target(&queue_row),
        "the Queue nav row must accept track drops"
    );
    let library_row = find_row(&shared, &ViewSource::Library).unwrap();
    assert!(
        !has_drop_target(&library_row),
        "the Library nav row must not accept drops"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn handle_queue_drop_dispatches_ids_to_the_wired_callback() {
    gtk4::init().unwrap();
    let shared = test_shared();
    let seen: Rc<RefCell<Vec<i64>>> = Rc::new(RefCell::new(Vec::new()));
    {
        let seen = seen.clone();
        *shared.on_queue_drop.borrow_mut() = Some(Rc::new(move |ids: &[i64]| {
            seen.borrow_mut().extend_from_slice(ids);
            true
        }));
    }

    assert!(crate::ui::sidebar_dnd::handle_queue_drop(&shared, &[7, 9]));
    assert_eq!(*seen.borrow(), vec![7, 9]);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn handle_queue_drop_is_a_noop_without_ids_or_callback() {
    gtk4::init().unwrap();
    let shared = test_shared();

    // No callback wired at all: report failure, don't panic.
    assert!(!crate::ui::sidebar_dnd::handle_queue_drop(&shared, &[7]));

    // Callback wired but empty ids: never invoked, reports failure.
    let invoked = Rc::new(Cell::new(false));
    {
        let invoked = invoked.clone();
        *shared.on_queue_drop.borrow_mut() = Some(Rc::new(move |_: &[i64]| {
            invoked.set(true);
            true
        }));
    }
    assert!(!crate::ui::sidebar_dnd::handle_queue_drop(&shared, &[]));
    assert!(!invoked.get());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fb_2a_progress_activity_is_pinned_to_the_sidebar_bottom() {
    gtk4::init().unwrap();
    let scrolled = gtk4::ScrolledWindow::builder().vexpand(true).build();
    let activity = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    activity.append(&gtk4::Label::new(Some("Cover check complete")));
    let issues = gtk4::ListBox::new();
    issues.append(&gtk4::Label::new(Some("Missing files")));
    let root = build_root(&scrolled, &activity, &issues);
    let window = gtk4::Window::builder()
        .default_width(300)
        .default_height(900)
        .child(&root)
        .build();

    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    // FB-2a: the shared progress activity is the sidebar's bottom slot.
    // Issues remain directly above it, so cover/scan/relink progress never
    // floats in the middle of a tall sidebar.
    let activity_bounds = activity.compute_bounds(&root).unwrap();
    let issues_bounds = issues.compute_bounds(&root).unwrap();
    assert_eq!(root.first_child().as_ref(), Some(scrolled.upcast_ref()));
    assert_eq!(scrolled.next_sibling().as_ref(), Some(issues.upcast_ref()));
    assert_eq!(root.last_child().as_ref(), Some(activity.upcast_ref()));
    assert!(
        (activity_bounds.y() + activity_bounds.height() - root.height() as f32).abs() < 0.5,
        "the progress activity must touch the sidebar bottom edge: activity={activity_bounds:?}, root_height={}",
        root.height()
    );
    assert!(
        (issues_bounds.y() + issues_bounds.height() - activity_bounds.y()).abs() < 0.5,
        "issues must sit directly above the progress activity: issues={issues_bounds:?}, activity={activity_bounds:?}"
    );
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn acc_3_sidebar_uses_the_available_page_height_before_scrolling() {
    gtk4::init().unwrap();
    let list = gtk4::ListBox::new();
    for label in [
        "Music",
        "Queue",
        "New playlist",
        "Import playlist",
        "Recently played",
        "Top rated",
        "Recently added",
        "My Stats",
    ] {
        list.append(&gtk4::Label::new(Some(label)));
    }
    let scrolled = build_navigation_scroller(&list);
    let activity = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let issues = gtk4::ListBox::new();
    issues.set_visible(false);
    let root = build_root(&scrolled, &activity, &issues);
    let page = adw::NavigationPage::builder()
        .title("Library")
        .child(&root)
        .build();
    let window = gtk4::Window::builder()
        .default_width(300)
        .default_height(900)
        .child(&page)
        .build();

    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    assert_eq!(
        root.height(),
        page.height(),
        "the sidebar root must consume the complete navigation page: root={}, page={}",
        root.height(),
        page.height()
    );
    assert_eq!(
        scrolled.height(),
        root.height(),
        "with no activity or issues, the navigation viewport must consume the sidebar: scrolled={}, root={}",
        scrolled.height(),
        root.height()
    );
    assert_eq!(
        scrolled.vscrollbar_policy(),
        gtk4::PolicyType::Automatic,
        "hiding the inert thumb must not disable future scrolling"
    );
    assert!(!scrolled.vscrollbar().is_visible());
    assert!(!scrolled.vscrollbar().can_target());
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn acc_3_short_sidebar_keeps_navigation_rows_scrollable() {
    gtk4::init().unwrap();
    let list = gtk4::ListBox::new();
    for index in 0..24 {
        list.append(&gtk4::Label::new(Some(&format!("Playlist {index}"))));
    }
    let scrolled = build_navigation_scroller(&list);
    let activity = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let issues = gtk4::ListBox::new();
    issues.set_visible(false);
    let root = build_root(&scrolled, &activity, &issues);
    let page = adw::NavigationPage::builder()
        .title("Library")
        .child(&root)
        .build();
    let window = gtk4::Window::builder()
        .default_width(300)
        .default_height(100)
        .child(&page)
        .build();

    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    let adjustment = scrolled.vadjustment();
    assert!(
        adjustment.upper() > adjustment.page_size(),
        "the fixture must overflow: upper={}, page_size={}",
        adjustment.upper(),
        adjustment.page_size()
    );
    assert_eq!(
        scrolled.vscrollbar_policy(),
        gtk4::PolicyType::Automatic,
        "a short sidebar must retain visible scrolling"
    );
    assert!(scrolled.vscrollbar().is_visible());
    assert!(scrolled.vscrollbar().can_target());
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn acc_3_bottom_pinned_issues_collection_is_a_tab_stop() {
    gtk4::init().unwrap();
    let issues = gtk4::ListBox::new();
    configure_issues_listbox(&issues);
    crate::ui::sidebar_presentation::append_problem_header(&issues);
    let row = crate::ui::sidebar_presentation::build_issue_nav_row(
        "Missing files",
        crate::ui::sidebar_presentation::issue_row_presentation(
            1,
            crate::ui::sidebar_presentation::NavIcon::Missing,
        ),
        crate::ui::sidebar_presentation::NavIcon::Missing,
    );
    issues.append(&row);

    remember_issue_focus_entry(&issues, &row);

    assert!(issues.is_focusable());
    assert_eq!(issues.focus_child().as_ref(), Some(row.upcast_ref()));

    let before = gtk4::Button::with_label("Before");
    let after = gtk4::Button::with_label("After");
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    root.append(&before);
    root.append(&issues);
    root.append(&after);
    let window = gtk4::Window::builder().child(&root).build();
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    assert!(before.grab_focus());
    assert!(window.child_focus(gtk4::DirectionType::TabForward));
    assert!(
        row.is_focus() || issues.is_focus(),
        "Tab must enter the separately pinned issues collection"
    );
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn focus_driven_selection_browses_without_routing_but_activation_routes() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let shared = test_shared();
    wire_row_selected(&shared);
    wire_row_activated(&shared);
    rebuild(&shared, None, "test build");
    let routed: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    {
        let routed = routed.clone();
        *shared.on_select.borrow_mut() = Some(Rc::new(move |source: ViewSource, _title| {
            routed.borrow_mut().push(source.label());
        }));
    }
    // Rows must be realized/mapped for focus to be grantable.
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    root.append(&shared.listbox);
    root.append(&shared.issues_listbox);
    let window = gtk4::Window::builder().child(&root).build();
    window.present();
    settle_until_active(&window);

    // Keyboard focus lands on the Queue row (Tab / arrow browsing) and
    // GTK's ListBox auto-selects it — that must NOT route (the optics run
    // caught tabbing THROUGH the sidebar yanking the app to the Queue).
    let queue_row = find_row(&shared, &ViewSource::Queue).unwrap();
    assert!(queue_row.grab_focus());
    shared.listbox.select_row(Some(&queue_row));
    assert!(
        routed.borrow().is_empty(),
        "focus-driven selection must not route"
    );

    // Committing (click / Enter / Space all emit `row-activated`) routes
    // exactly once — the single-click navigation contract.
    queue_row.emit_by_name::<()>("activate", &[]);
    assert_eq!(*routed.borrow(), vec![ViewSource::Queue.label()]);

    // Programmatic selection (refresh_and_select's path — no focus on the
    // row) still routes.
    gtk4::prelude::GtkWindowExt::set_focus(&window, gtk4::Widget::NONE);
    let library_row = find_row(&shared, &ViewSource::Library).unwrap();
    shared.listbox.select_row(Some(&library_row));
    assert_eq!(
        *routed.borrow(),
        vec![ViewSource::Queue.label(), ViewSource::Library.label()]
    );
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn acc_3_focus_transfer_between_sidebar_collections_does_not_resync_mid_flight() {
    gtk4::init().unwrap();
    let shared = test_shared();
    wire_row_selected(&shared);
    wire_focus_leave_resync(&shared);
    rebuild(&shared, None, "test build");
    let missing = crate::ui::sidebar_presentation::build_issue_nav_row(
        "Missing files",
        crate::ui::sidebar_presentation::issue_row_presentation(
            1,
            crate::ui::sidebar_presentation::NavIcon::Missing,
        ),
        crate::ui::sidebar_presentation::NavIcon::Missing,
    );
    shared.issues_listbox.append(&missing);
    remember_issue_focus_entry(&shared.issues_listbox, &missing);
    shared.rows.borrow_mut().push((
        missing.clone(),
        ViewSource::Missing,
        "Missing files".to_string(),
    ));

    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    root.append(&shared.listbox);
    root.append(&shared.issues_listbox);
    let window = gtk4::Window::builder().child(&root).build();
    window.present();
    // `has_focus()` — which `wire_row_selected_on`'s no-route guard consults —
    // additionally requires an active toplevel, and X delivers activation a few
    // milliseconds after `present()`. Without this wait the guard never fires
    // and the assertions below would pass through the unconditional
    // `unselect_all()` instead of the behaviour they name.
    settle_until_active(&window);

    let queue = find_row(&shared, &ViewSource::Queue).unwrap();
    assert!(queue.grab_focus());
    shared.listbox.select_row(Some(&queue));
    assert!(missing.grab_focus());
    shared.issues_listbox.select_row(Some(&missing));
    while gtk4::glib::MainContext::default().iteration(false) {}

    assert!(missing.is_focus());
    assert_eq!(
        shared.issues_listbox.selected_row().as_ref(),
        Some(&missing)
    );
    assert!(shared.listbox.selected_row().is_none());
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn acc_3_sidebar_collection_boundaries_link_main_and_issues() {
    gtk4::init().unwrap();
    let shared = test_shared();
    rebuild(&shared, None, "test build");
    let missing = crate::ui::sidebar_presentation::build_issue_nav_row(
        "Missing files",
        crate::ui::sidebar_presentation::issue_row_presentation(
            1,
            crate::ui::sidebar_presentation::NavIcon::Missing,
        ),
        crate::ui::sidebar_presentation::NavIcon::Missing,
    );
    shared.issues_listbox.append(&missing);
    shared.rows.borrow_mut().push((
        missing.clone(),
        ViewSource::Missing,
        "Missing files".to_string(),
    ));

    assert_eq!(first_issue_row(&shared).as_ref(), Some(&missing));
    assert!(last_main_row(&shared).is_some());
    assert_eq!(
        last_main_row(&shared).unwrap().parent().as_ref(),
        Some(shared.listbox.upcast_ref())
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn smart_playlist_rows_badge_their_live_track_count() {
    gtk4::init().unwrap();
    let shared = test_shared();

    // Seed five present tracks; the default "Recently added" smart list has an
    // empty rule set, so it matches every present track — the badge must read 5.
    let smart_id = {
        let conn = shared.conn.borrow();
        for id in 1..=5i64 {
            conn.execute(
                "INSERT INTO tracks (path, title, artist, added_at) \
                 VALUES (?1, ?2, 'Synthetic Artist', ?3)",
                (
                    format!("/synthetic/{id:03}.flac"),
                    format!("Track {id:03}"),
                    id,
                ),
            )
            .unwrap();
        }
        conn.query_row(
            "SELECT id FROM smart_playlists WHERE name = 'Recently added'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
    };

    rebuild(&shared, None, "test build");

    let row = find_row(&shared, &ViewSource::Smart(smart_id))
        .expect("the 'Recently added' smart list must have a sidebar row");
    assert_eq!(
        numeric_badge_text(row.upcast_ref()),
        Some("5".to_string()),
        "the smart list must badge its live track count like a manual playlist"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn empty_smart_playlist_shows_no_badge() {
    gtk4::init().unwrap();
    let shared = test_shared();

    // No tracks seeded: every default smart list resolves to zero and must
    // therefore render no badge at all (nonzero-only policy).
    let smart_id = {
        let conn = shared.conn.borrow();
        conn.query_row(
            "SELECT id FROM smart_playlists WHERE name = 'Recently added'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
    };

    rebuild(&shared, None, "test build");

    let row = find_row(&shared, &ViewSource::Smart(smart_id))
        .expect("the 'Recently added' smart list must have a sidebar row");
    assert_eq!(
        numeric_badge_text(row.upcast_ref()),
        None,
        "a zero-count smart list must not render a literal-zero badge"
    );
}

#[test]
fn keeps_requested_source_when_its_row_still_exists() {
    let (source, fell_back) = resolve_select_source(ViewSource::Playlist(3), true);
    assert_eq!(source, ViewSource::Playlist(3));
    assert!(!fell_back);
}

#[test]
fn falls_back_to_library_when_requested_row_is_gone() {
    let (source, fell_back) = resolve_select_source(ViewSource::Missing, false);
    assert_eq!(source, ViewSource::Library);
    assert!(fell_back);
}

#[test]
fn falls_back_to_library_when_a_smart_list_vanished() {
    let (source, fell_back) = resolve_select_source(ViewSource::Smart(7), false);
    assert_eq!(source, ViewSource::Library);
    assert!(fell_back);
}

#[test]
fn restored_source_reuses_the_vanished_source_fallback() {
    assert_eq!(
        resolve_select_source(ViewSource::Playlist(99), false).0,
        ViewSource::Library
    );
    assert_eq!(
        resolve_select_source(ViewSource::Queue, true).0,
        ViewSource::Queue
    );
}

// UX INST-13: the instrumental conversions view is reachable from the sidebar
// only while the experimental switch is on (INST-11) — the row appears when the
// switch is on and is absent when it is off.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn inst_13_experimental_switch_reveals_the_conversions_sidebar_row() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let shared = test_shared();

    // Off by default: no conversions row.
    rebuild(&shared, None, "test build");
    assert!(
        find_row(&shared, &ViewSource::Conversions).is_none(),
        "the conversions row is hidden while experimental is off"
    );

    // On: the row appears after a rebuild.
    crate::ui::instrumental::set_experimental_enabled(&shared.conn.borrow(), true).unwrap();
    rebuild(&shared, None, "experimental enabled");
    assert!(
        find_row(&shared, &ViewSource::Conversions).is_some(),
        "the conversions row appears once experimental is on (INST-13)"
    );
}

fn assert_update_feed_rows_are_module_gated_ordered_and_badged() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let shared = test_shared();

    rebuild(&shared, None, "modules off");
    assert!(find_row(&shared, &ViewSource::Releases).is_none());
    assert!(find_row(&shared, &ViewSource::Concerts).is_none());

    {
        let conn = shared.conn.borrow();
        reprise_core::modules::set_enabled(
            &conn,
            &reprise_core::modules::NEW_RELEASES_MODULE,
            true,
        )
        .unwrap();
        reprise_core::modules::set_enabled(&conn, &reprise_core::modules::CONCERTS_MODULE, true)
            .unwrap();
        conn.execute(
            "INSERT INTO new_releases (
               release_group_mbid, artist_name, artist_mbid, title, release_type,
               first_release_date, fetched_at, fallback_accent, first_seen
             ) VALUES ('release', 'Artist', 'artist-id', 'Release', 'Album',
                       '2099-08-01', 1, '#123456', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO concert_events (
               artist_key, artist_name, starts_at, date_key, venue, city,
               provider, fetched_at, dedupe_key
             ) VALUES ('artist', 'Artist', '2099-08-02T20:00:00',
                       '2099-08-02', 'Venue', 'City', 'bandsintown', 1,
                       '2099-08-02|city|venue')",
            [],
        )
        .unwrap();
    }

    rebuild(&shared, None, "modules on");
    let rows = shared.rows.borrow();
    let releases = rows
        .iter()
        .position(|(_, source, _)| matches!(source, ViewSource::Releases))
        .unwrap();
    let concerts = rows
        .iter()
        .position(|(_, source, _)| matches!(source, ViewSource::Concerts))
        .unwrap();
    let stats = rows
        .iter()
        .position(|(_, source, _)| matches!(source, ViewSource::MyStats))
        .unwrap();
    assert!(releases < concerts && concerts < stats);
    assert_eq!(
        numeric_badge_text(rows[releases].0.upcast_ref()),
        Some("1".to_string())
    );
    assert_eq!(
        numeric_badge_text(rows[concerts].0.upcast_ref()),
        Some("1".to_string())
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn conc_1_concerts_row_is_module_gated_and_badged_from_the_filtered_view() {
    assert_update_feed_rows_are_module_gated_ordered_and_badged();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nr_15_releases_row_is_module_gated_before_concerts_and_badged_from_the_filtered_view() {
    assert_update_feed_rows_are_module_gated_ordered_and_badged();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_1_podcast_and_radio_rows_are_gated_ordered_and_live_counted() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let shared = test_shared();

    rebuild(&shared, None, "source defaults");
    assert!(find_row(&shared, &ViewSource::Podcasts).is_none());
    assert!(find_row(&shared, &ViewSource::Radio).is_some());

    {
        let conn = shared.conn.borrow();
        reprise_core::modules::set_enabled(&conn, &reprise_core::modules::PODCASTS_MODULE, true)
            .unwrap();
        conn.execute(
            "INSERT INTO podcast_subscriptions
               (kind, feed_url, title, auto_download, added_at)
             VALUES ('rss', 'https://example.test/feed', 'Show', 0, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO podcast_episodes
               (subscription_id, guid, title, audio_url, position_ms, first_seen_at)
             VALUES (1, 'episode', 'Episode', 'https://example.test/episode.mp3', 0, 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO radio_stations (name, stream_url, added_at)
             VALUES ('Station', 'https://example.test/live', 1)",
            [],
        )
        .unwrap();
    }

    rebuild(&shared, None, "source data changed");
    let rows = shared.rows.borrow();
    let music = rows
        .iter()
        .position(|(_, source, _)| matches!(source, ViewSource::Library))
        .unwrap();
    let podcasts = rows
        .iter()
        .position(|(_, source, _)| matches!(source, ViewSource::Podcasts))
        .unwrap();
    let radio = rows
        .iter()
        .position(|(_, source, _)| matches!(source, ViewSource::Radio))
        .unwrap();
    let queue = rows
        .iter()
        .position(|(_, source, _)| matches!(source, ViewSource::Queue))
        .unwrap();
    assert!(music < podcasts && podcasts < radio && radio < queue);
    assert_eq!(
        numeric_badge_text(rows[podcasts].0.upcast_ref()),
        Some("1".to_string())
    );
    assert_eq!(
        numeric_badge_text(rows[radio].0.upcast_ref()),
        Some("1".to_string())
    );
}
