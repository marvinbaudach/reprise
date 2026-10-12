//! Display-dependent proof of which "Rules for this phone" switches lock while
//! a sync is running (`MTP-30`, `MTP-33`). Kept in its own file so the page
//! display tests stay under the project's 800-line limit.

use super::*;

fn page_actions() -> PageActions {
    PageActions {
        set_profile: Rc::new(|_| {}),
        set_playlist: Rc::new(|_, _| {}),
        start: Rc::new(|| {}),
        cancel: Rc::new(|| {}),
        eject: Rc::new(|| {}),
    }
}

fn rule_switches(section: &OnDeviceSection) -> Vec<gtk4::Switch> {
    fn collect(widget: &gtk4::Widget, found: &mut Vec<gtk4::Switch>) {
        if let Ok(switch) = widget.clone().downcast::<gtk4::Switch>() {
            found.push(switch);
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            collect(&current, found);
            child = current.next_sibling();
        }
    }
    let mut found = Vec::new();
    collect(section.root().upcast_ref(), &mut found);
    found
}

fn busy_device() -> DeviceView {
    let mut busy = device();
    busy.sync_phase = PlannedSyncPhase::Finishing;
    busy.page.update_controls(true, true, true);
    busy
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mtp_69_the_remove_deleted_switch_is_locked_while_a_sync_runs() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().expect("GTK test display");
    let (surface, _root) = DeviceSyncPage::new(&device(), page_actions(), &no_op_content_actions());
    surface.update(&busy_device());

    let switches = rule_switches(&surface.on_device);
    assert_eq!(switches.len(), 2);
    assert!(!switches[0].is_sensitive(), "remove-deleted must lock");
    assert!(switches[0]
        .tooltip_text()
        .is_some_and(|tip| tip.contains("locked")));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mtp_69_the_automatic_sync_switch_stays_editable_while_a_sync_runs() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().expect("GTK test display");
    let (surface, _root) = DeviceSyncPage::new(&device(), page_actions(), &no_op_content_actions());
    surface.update(&busy_device());

    let switches = rule_switches(&surface.on_device);
    assert!(
        switches[1].is_sensitive(),
        "automatic sync must stay editable"
    );
    assert!(switches[1].tooltip_text().is_none());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mtp_69_the_remove_deleted_switch_unlocks_when_the_sync_ends() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().expect("GTK test display");
    let (surface, _root) = DeviceSyncPage::new(&device(), page_actions(), &no_op_content_actions());
    surface.update(&busy_device());
    surface.update(&device());

    let switches = rule_switches(&surface.on_device);
    assert!(switches[0].is_sensitive());
    assert!(switches[0].tooltip_text().is_none());
}
