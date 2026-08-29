use super::*;
use crate::ui::device_sync_runtime::{PlannedSyncPhase, SyncStep};
use crate::ui::sidebar::sidebar_device_card::tests::view;
use crate::ui::sidebar::sidebar_place::{apply_marking, find_row, SidebarPlace};
use crate::ui::sidebar::sidebar_rebuild::rebuild;
use crate::ui::sidebar::sidebar_row_wiring::{wire_row_activated, wire_row_selected};
use crate::ui::sidebar::surface::test_shared;
use reprise_core::view_source::ViewSource;

fn two_cards() -> CardRegistry {
    let on_open: OpenCallback = Rc::new(|_, _| {});
    let on_cancel: CancelCallback = Rc::new(|_| {});
    let pixel = view(PlannedSyncPhase::Idle);
    let mut tablet = view(PlannedSyncPhase::Idle);
    tablet.id = "tablet".into();
    tablet.name = "Tablet".into();

    Rc::new(RefCell::new(HashMap::from([
        (
            pixel.id.clone(),
            DeviceCard::new(&pixel, &on_open, &on_cancel),
        ),
        (
            tablet.id.clone(),
            DeviceCard::new(&tablet, &on_open, &on_cancel),
        ),
    ])))
}

fn install_marker(shared: &Rc<Shared>, cards: &CardRegistry) {
    let cards = cards.clone();
    *shared.mark_device.borrow_mut() = Some(Rc::new(move |current_id| {
        apply_current(&cards.borrow(), current_id);
    }));
}

fn assert_current(card: &DeviceCard, current: bool) {
    assert_eq!(card.surface().has_css_class("device-card-current"), current);
    if current {
        assert!(gtk4::test_accessible_has_state(
            card.surface(),
            gtk4::AccessibleState::Selected
        ));
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_18_only_the_open_device_card_is_marked() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    crate::ui::style::install();
    let shared = test_shared();
    let cards = two_cards();
    install_marker(&shared, &cards);

    *shared.current_place.borrow_mut() = SidebarPlace::Device("pixel".into());
    apply_marking(&shared);

    let cards = cards.borrow();
    assert_current(&cards["pixel"], true);
    assert_current(&cards["tablet"], false);
    assert!(shared.listbox.selected_row().is_none());
    assert!(shared.issues_listbox.selected_row().is_none());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_18_a_sync_progress_update_keeps_the_open_device_marked() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    crate::ui::style::install();
    let shared = test_shared();
    let cards = two_cards();
    install_marker(&shared, &cards);
    *shared.current_place.borrow_mut() = SidebarPlace::Device("pixel".into());
    apply_marking(&shared);

    let syncing = PlannedSyncPhase::Syncing {
        done: 1,
        total: 4,
        current_track: "Song".into(),
        unit_bytes_done: 256,
        unit_bytes_total: 1_024,
        step: SyncStep::Copying,
    };
    let cards = cards.borrow();
    cards["pixel"].update(&view(syncing));

    assert_current(&cards["pixel"], true);
    assert!(cards["pixel"].surface().has_css_class("device-card-active"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_18_activating_a_source_from_the_device_page_routes_back_and_unmarks_the_card() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    crate::ui::style::install();
    let shared = test_shared();
    *shared.current_source.borrow_mut() = ViewSource::Library;
    wire_row_selected(&shared);
    wire_row_activated(&shared);
    rebuild(&shared, Some(ViewSource::Library), "device route fixture");

    let cards = two_cards();
    install_marker(&shared, &cards);
    let stack = gtk4::Stack::new();
    stack.add_named(&gtk4::Label::new(Some("Library")), Some("library"));
    stack.add_named(&gtk4::Label::new(Some("Device")), Some("device-sync"));
    stack.set_visible_child_name("library");
    shared.content_stack.set(Some(&stack));

    let routed = Rc::new(RefCell::new(Vec::new()));
    let routed_for_callback = routed.clone();
    let stack_for_source = stack.clone();
    *shared.on_select.borrow_mut() = Some(Rc::new(move |source, _| {
        routed_for_callback.borrow_mut().push(source);
        crate::ui::window::content_stack::show_page(&stack_for_source, "library");
    }));
    let stack_for_device = stack.clone();
    let open: OpenCallback = Rc::new(move |_, _| {
        crate::ui::window::content_stack::show_page(&stack_for_device, "device-sync");
    });

    track_open_device(&shared, open)("pixel".into(), "Pixel 8".into());
    crate::ui::sidebar::sync_place_from_stack(&shared);
    assert_eq!(stack.visible_child_name().as_deref(), Some("device-sync"));
    assert_current(&cards.borrow()["pixel"], true);

    let library_row = find_row(&shared, &ViewSource::Library).unwrap();
    library_row.emit_by_name::<()>("activate", &[]);
    while gtk4::glib::MainContext::default().iteration(false) {}

    assert_eq!(stack.visible_child_name().as_deref(), Some("library"));
    assert_eq!(*routed.borrow(), vec![ViewSource::Library]);
    assert_eq!(shared.listbox.selected_row().as_ref(), Some(&library_row));
    assert!(shared.issues_listbox.selected_row().is_none());
    assert_current(&cards.borrow()["pixel"], false);
}
