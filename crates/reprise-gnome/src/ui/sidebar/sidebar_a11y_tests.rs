use super::resolve_select_source_tests::test_shared;
use super::*;

fn navigation_button(row: &gtk4::ListBoxRow) -> gtk4::Button {
    row.child()
        .expect("navigation row has a child")
        .downcast::<gtk4::Button>()
        .expect("navigation row child is a real GtkButton")
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_11_sidebar_button_and_row_activation_share_the_production_route() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let shared = test_shared();
    wire_row_selected(&shared);
    wire_row_activated(&shared);
    rebuild(&shared, None, "test build");
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    root.append(&shared.listbox);
    root.append(&shared.issues_listbox);
    let window = gtk4::Window::builder().child(&root).build();
    window.present();

    let routed: Rc<RefCell<Vec<ViewSource>>> = Rc::new(RefCell::new(Vec::new()));
    {
        let routed = routed.clone();
        *shared.on_select.borrow_mut() = Some(Rc::new(move |source, _| {
            routed.borrow_mut().push(source);
        }));
    }
    let shown = Rc::new(Cell::new(0));
    {
        let shown = shown.clone();
        *shared.on_show_content.borrow_mut() = Some(Rc::new(move || shown.set(shown.get() + 1)));
    }

    let queue = find_row(&shared, &ViewSource::Queue).unwrap();
    navigation_button(&queue).emit_clicked();
    assert_eq!(shared.listbox.selected_row().as_ref(), Some(&queue));

    let library = find_row(&shared, &ViewSource::Library).unwrap();
    library.emit_by_name::<()>("activate", &[]);

    assert_eq!(
        *routed.borrow(),
        vec![ViewSource::Queue, ViewSource::Library],
        "the real button click and native row activation must traverse route_row"
    );
    assert_eq!(shown.get(), 2);
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_11_issue_button_activates_the_production_window_action() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let shared = test_shared();
    let activated = Rc::new(Cell::new(false));
    let action = gtk4::gio::SimpleAction::new("library-doctor-findings", None);
    action.connect_activate({
        let activated = activated.clone();
        move |_, _| activated.set(true)
    });
    let actions = gtk4::gio::SimpleActionGroup::new();
    actions.add_action(&action);
    shared
        .issues_listbox
        .insert_action_group("win", Some(&actions));

    super::super::sidebar_rebuild::add_issue_action_row(
        &shared,
        "Library Doctor",
        2,
        super::super::sidebar_presentation::NavIcon::LibraryDoctor,
        "win.library-doctor-findings",
    );
    let window = gtk4::Window::builder()
        .child(&shared.issues_listbox)
        .build();
    window.present();
    let row = shared.issues_listbox.row_at_index(0).unwrap();
    navigation_button(&row).emit_clicked();

    assert!(activated.get());
    window.close();
}

/// Every widget AT-SPI publishes below `root`, as `(role, widget)` pairs.
///
/// GTK's AT-SPI backend prunes a widget whose role is `Presentation` or
/// `None` together with its whole subtree. The GTK-side accessible tree keeps
/// those nodes, so a bare role assertion cannot see the pruning; this walk
/// applies the same rule, and an Atspi walk of the running app confirmed it.
///
/// The rule mirrors GTK 4.22.4. It is a model of the backend, not the backend:
/// if GTK changes how it prunes, a real AT-SPI walk of an isolated run (see
/// the NAV-11 proof in the PR that introduced this test) is the check, and this
/// walk has to follow it.
fn exposed_widgets(root: &gtk4::Accessible) -> Vec<(gtk4::AccessibleRole, gtk4::Accessible)> {
    let mut exposed = Vec::new();
    let mut pending = vec![root.clone()];
    while let Some(node) = pending.pop() {
        let role = node.accessible_role();
        if matches!(
            role,
            gtk4::AccessibleRole::Presentation | gtk4::AccessibleRole::None
        ) {
            continue;
        }
        let mut child = node.first_accessible_child();
        while let Some(current) = child {
            child = current.next_accessible_sibling();
            pending.push(current);
        }
        exposed.push((role, node));
    }
    exposed
}

fn presented_sidebar() -> (Rc<Shared>, gtk4::Window) {
    let shared = test_shared();
    rebuild(&shared, None, "test build");
    let window = gtk4::Window::builder().child(&shared.listbox).build();
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}
    (shared, window)
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_11_section_headings_are_published_to_assistive_technology() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (shared, window) = presented_sidebar();

    let exposed = exposed_widgets(shared.listbox.upcast_ref());
    let headings = exposed
        .iter()
        .filter(|(role, _)| *role == gtk4::AccessibleRole::Heading)
        .collect::<Vec<_>>();
    let mut names = headings
        .iter()
        .filter_map(|(_, widget)| widget.downcast_ref::<gtk4::Label>())
        .map(|label| label.text().to_string())
        .collect::<Vec<_>>();
    names.sort();
    let mut expected = [
        crate::ui::strings::SIDEBAR_SECTION_LIBRARY,
        crate::ui::strings::SIDEBAR_SECTION_PLAYLISTS,
        crate::ui::strings::SIDEBAR_SECTION_SMART,
    ]
    .map(crate::ui::strings::text)
    .to_vec();
    expected.sort();
    assert_eq!(
        names, expected,
        "every section heading must be reachable in the accessibility tree"
    );

    for (_, widget) in headings {
        let row = widget
            .downcast_ref::<gtk4::Widget>()
            .and_then(|heading| heading.ancestor(gtk4::ListBoxRow::static_type()))
            .and_downcast::<gtk4::ListBoxRow>()
            .expect("a heading sits inside a sidebar row");
        assert!(
            !row.is_selectable() && !row.is_activatable() && !row.is_focusable(),
            "a heading row stays non-operable"
        );
    }
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_11_new_playlist_button_is_published_as_a_named_button() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (shared, window) = presented_sidebar();
    let button = shared
        .playlist_add_button
        .borrow()
        .clone()
        .expect("the sidebar builds the new-playlist button");

    let exposed = exposed_widgets(shared.listbox.upcast_ref());
    assert!(
        exposed.iter().any(|(role, widget)| {
            *role == gtk4::AccessibleRole::Button
                && widget.downcast_ref::<gtk4::Button>() == Some(&button)
        }),
        "the new-playlist button must be reachable in the accessibility tree"
    );
    let expected = crate::ui::strings::text(crate::ui::strings::SIDEBAR_NEW_PLAYLIST);
    assert!(!expected.is_empty(), "the intended name is not empty");
    assert_eq!(
        crate::ui::test_accessible_label::accessible_label_mismatch(&button, &expected),
        None,
        "the new-playlist button's accessible label must be its translated name"
    );
    assert!(button.is_focusable());
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_11_every_navigation_row_stays_published_beside_the_headings() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (shared, window) = presented_sidebar();

    let exposed = exposed_widgets(shared.listbox.upcast_ref());
    let list_items = exposed
        .iter()
        .filter(|(role, _)| *role == gtk4::AccessibleRole::ListItem)
        .count();
    let rows = shared.rows.borrow().len();
    assert!(rows > 0, "the sidebar builds navigation rows");
    assert_eq!(
        list_items, rows,
        "headings must not become list items or hide navigation rows"
    );
    window.close();
}
