//! Shared home for long-running sidebar activity, directly above the
//! bottom-pinned Issues list so progress grows upward (FB-2a).
//!
//! Scan and device sync own their cards and update loops. This module owns
//! only their stable layout relationship, so construction order cannot move
//! either activity out of the slot or reorder the two relative to each other.

use std::cell::RefCell;

use gtk4::prelude::*;

pub(super) struct SidebarActivitySlot {
    root: gtk4::Box,
    device_section: RefCell<Option<gtk4::Widget>>,
    scan_card: RefCell<Option<gtk4::Widget>>,
    doctor_card: RefCell<Option<gtk4::Widget>>,
    relink_card: RefCell<Option<gtk4::Widget>>,
}

impl SidebarActivitySlot {
    pub(super) fn new() -> Self {
        Self {
            root: gtk4::Box::new(gtk4::Orientation::Vertical, 0),
            device_section: RefCell::new(None),
            scan_card: RefCell::new(None),
            doctor_card: RefCell::new(None),
            relink_card: RefCell::new(None),
        }
    }

    pub(super) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub(super) fn set_device_section(&self, section: &impl IsA<gtk4::Widget>) {
        replace_child(&self.root, &self.device_section, section);
        let section = section.upcast_ref::<gtk4::Widget>();
        self.root
            .reorder_child_after(section, None::<&gtk4::Widget>);
    }

    pub(super) fn set_scan_card(&self, card: &impl IsA<gtk4::Widget>) {
        replace_child(&self.root, &self.scan_card, card);
        let card = card.upcast_ref::<gtk4::Widget>();
        let device = self.device_section.borrow().clone();
        self.root.reorder_child_after(card, device.as_ref());
    }

    pub(super) fn set_relink_card(&self, card: &impl IsA<gtk4::Widget>) {
        replace_child(&self.root, &self.relink_card, card);
        let card = card.upcast_ref::<gtk4::Widget>();
        let predecessor = self
            .doctor_card
            .borrow()
            .clone()
            .or_else(|| self.scan_card.borrow().clone())
            .or_else(|| self.device_section.borrow().clone());
        self.root.reorder_child_after(card, predecessor.as_ref());
    }

    pub(super) fn set_doctor_card(&self, card: &impl IsA<gtk4::Widget>) {
        replace_child(&self.root, &self.doctor_card, card);
        let card = card.upcast_ref::<gtk4::Widget>();
        let predecessor = self
            .scan_card
            .borrow()
            .clone()
            .or_else(|| self.device_section.borrow().clone());
        self.root.reorder_child_after(card, predecessor.as_ref());
    }
}

impl super::Sidebar {
    /// Places the scan-progress card in the shared bottom activity slot.
    /// Called once at window build time (after sidebar and scan controls exist).
    pub fn append_scan_card(&self, widget: &impl IsA<gtk4::Widget>) {
        self.activity_slot.set_scan_card(widget);
    }

    /// Places Locate's relink-search card in the same bottom activity slot.
    pub fn append_relink_card(&self, widget: &impl IsA<gtk4::Widget>) {
        self.activity_slot.set_relink_card(widget);
    }

    /// Places the one Library Doctor job card in the shared activity slot.
    pub fn append_doctor_card(&self, widget: &impl IsA<gtk4::Widget>) {
        self.activity_slot.set_doctor_card(widget);
    }
}

fn replace_child(
    root: &gtk4::Box,
    stored: &RefCell<Option<gtk4::Widget>>,
    child: &impl IsA<gtk4::Widget>,
) {
    if let Some(previous) = stored.borrow_mut().replace(child.clone().upcast()) {
        root.remove(&previous);
    }
    root.append(child);
}

#[cfg(test)]
mod tests {
    use gtk4::prelude::*;

    use super::SidebarActivitySlot;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn device_and_scan_activity_stack_in_stable_bottom_slot_order() {
        if gtk4::init().is_err() {
            return;
        }

        let slot = SidebarActivitySlot::new();
        let device = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let scan = gtk4::Revealer::builder()
            .child(&gtk4::Label::new(Some("Scanning")))
            .build();
        let relink = gtk4::Revealer::builder()
            .child(&gtk4::Label::new(Some("Relinking")))
            .build();
        let doctor = gtk4::Revealer::builder()
            .child(&gtk4::Label::new(Some("Library Doctor")))
            .build();

        // Attach in the reverse of window construction order to prove that
        // callers cannot accidentally move either activity out of place.
        slot.set_relink_card(&relink);
        slot.set_doctor_card(&doctor);
        slot.set_scan_card(&scan);
        slot.set_device_section(&device);

        assert_eq!(
            slot.widget().first_child().as_ref(),
            Some(device.upcast_ref())
        );
        assert_eq!(
            slot.widget().last_child().as_ref(),
            Some(relink.upcast_ref())
        );

        device.set_visible(true);
        scan.set_reveal_child(true);
        relink.set_reveal_child(true);
        doctor.set_reveal_child(true);
        assert!(device.is_visible());
        assert!(scan.reveals_child());
        assert!(relink.reveals_child());
        assert!(doctor.reveals_child());
    }
}
