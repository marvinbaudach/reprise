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

fn has_drop_target(row: &gtk4::ListBoxRow) -> bool {
    let controllers = row.observe_controllers();
    (0..controllers.n_items()).any(|index| {
        controllers
            .item(index)
            .is_some_and(|controller| controller.is::<gtk4::DropTarget>())
    })
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
fn issues_list_is_the_bottom_most_root_child_below_the_activity_slot() {
    gtk4::init().unwrap();
    let scrolled = gtk4::ScrolledWindow::new();
    let activity = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let issues = gtk4::ListBox::new();
    let root = build_root(&scrolled, &activity, &issues);

    // The scrolling nav list stays on top and the issues list is pinned at
    // the very bottom (QA #6), with the activity slot sandwiched between so an
    // active scan/relink card grows upward instead of pushing issues off the
    // bottom edge.
    assert_eq!(root.first_child().as_ref(), Some(scrolled.upcast_ref()));
    assert_eq!(
        scrolled.next_sibling().as_ref(),
        Some(activity.upcast_ref())
    );
    assert_eq!(root.last_child().as_ref(), Some(issues.upcast_ref()));
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
        row.has_focus() || issues.has_focus(),
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
    while gtk4::glib::MainContext::default().iteration(false) {}

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
    while gtk4::glib::MainContext::default().iteration(false) {}

    let queue = find_row(&shared, &ViewSource::Queue).unwrap();
    assert!(queue.grab_focus());
    shared.listbox.select_row(Some(&queue));
    assert!(missing.grab_focus());
    shared.issues_listbox.select_row(Some(&missing));
    while gtk4::glib::MainContext::default().iteration(false) {}

    assert!(missing.has_focus());
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
