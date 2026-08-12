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
