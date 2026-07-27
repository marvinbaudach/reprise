//! Shared home for sidebar device state and replacement progress cards.
//!
//! Scan and device sync own their cards and update loops. This module owns
//! only their stable layout relationship, so construction order cannot move
//! either activity out of the slot or reorder the two relative to each other.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;

pub(super) struct SidebarActivitySlot {
    /// Persistent device state remains independent from Issues/progress.
    root: gtk4::Box,
    /// Long-running cards temporarily replace Issues while any is visible.
    progress_root: gtk4::Box,
    progress_spacer: gtk4::Box,
    device_section: RefCell<Option<gtk4::Widget>>,
    scan_card: RefCell<Option<gtk4::Widget>>,
    doctor_card: RefCell<Option<gtk4::Widget>>,
    relink_card: RefCell<Option<gtk4::Widget>>,
    issues_stack: Rc<RefCell<gtk4::glib::WeakRef<gtk4::Stack>>>,
    progress_cards: Rc<RefCell<Vec<gtk4::glib::WeakRef<gtk4::Widget>>>>,
}

impl SidebarActivitySlot {
    pub(super) fn new() -> Self {
        let issues_stack = gtk4::glib::WeakRef::new();
        let progress_root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let progress_spacer = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        progress_spacer.set_vexpand(true);
        progress_root.append(&progress_spacer);
        Self {
            root: gtk4::Box::new(gtk4::Orientation::Vertical, 0),
            progress_root,
            progress_spacer,
            device_section: RefCell::new(None),
            scan_card: RefCell::new(None),
            doctor_card: RefCell::new(None),
            relink_card: RefCell::new(None),
            issues_stack: Rc::new(RefCell::new(issues_stack)),
            progress_cards: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub(super) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub(super) fn progress_widget(&self) -> &gtk4::Box {
        &self.progress_root
    }

    pub(super) fn set_device_section(&self, section: &impl IsA<gtk4::Widget>) {
        replace_child(&self.root, &self.device_section, section);
        let section = section.upcast_ref::<gtk4::Widget>();
        self.root
            .reorder_child_after(section, None::<&gtk4::Widget>);
    }

    pub(super) fn set_scan_card(&self, card: &impl IsA<gtk4::Widget>) {
        replace_child(&self.progress_root, &self.scan_card, card);
        let card = card.upcast_ref::<gtk4::Widget>();
        self.progress_root.reorder_child_after(
            card,
            Some(self.progress_spacer.upcast_ref::<gtk4::Widget>()),
        );
        self.track_progress_visibility(card);
    }

    pub(super) fn set_relink_card(&self, card: &impl IsA<gtk4::Widget>) {
        replace_child(&self.progress_root, &self.relink_card, card);
        let card = card.upcast_ref::<gtk4::Widget>();
        let predecessor = self
            .doctor_card
            .borrow()
            .clone()
            .or_else(|| self.scan_card.borrow().clone())
            .or_else(|| Some(self.progress_spacer.clone().upcast()));
        self.progress_root
            .reorder_child_after(card, predecessor.as_ref());
        self.track_progress_visibility(card);
    }

    pub(super) fn set_doctor_card(&self, card: &impl IsA<gtk4::Widget>) {
        replace_child(&self.progress_root, &self.doctor_card, card);
        let card = card.upcast_ref::<gtk4::Widget>();
        let predecessor = self
            .scan_card
            .borrow()
            .clone()
            .or_else(|| Some(self.progress_spacer.clone().upcast()));
        self.progress_root
            .reorder_child_after(card, predecessor.as_ref());
        self.track_progress_visibility(card);
    }

    pub(super) fn attach_issues_stack(&self, stack: &gtk4::Stack) {
        self.issues_stack.borrow().set(Some(stack));
        self.show_surface_for_progress();
    }

    fn track_progress_visibility(&self, card: &gtk4::Widget) {
        let weak = gtk4::glib::WeakRef::new();
        weak.set(Some(card));
        self.progress_cards.borrow_mut().push(weak);
        let issues_stack = self.issues_stack.clone();
        let progress_cards = self.progress_cards.clone();
        let refresh: Rc<dyn Fn()> = Rc::new(move || {
            show_surface_for_progress(&issues_stack, &progress_cards);
        });
        card.connect_visible_notify({
            let refresh = refresh.clone();
            move |_| refresh()
        });
        if let Some(revealer) = card.downcast_ref::<gtk4::Revealer>() {
            revealer.connect_reveal_child_notify({
                let refresh = refresh.clone();
                move |revealer| {
                    sync_revealer_visibility(revealer);
                    refresh();
                }
            });
            revealer.connect_child_revealed_notify(move |revealer| {
                sync_revealer_visibility(revealer);
                refresh();
            });
            sync_revealer_visibility(revealer);
        }
        self.show_surface_for_progress();
    }

    fn show_surface_for_progress(&self) {
        show_surface_for_progress(&self.issues_stack, &self.progress_cards);
    }
}

fn sync_revealer_visibility(revealer: &gtk4::Revealer) {
    let should_be_visible = revealer.reveals_child() || revealer.is_child_revealed();
    if revealer.is_visible() != should_be_visible {
        revealer.set_visible(should_be_visible);
    }
}

fn show_surface_for_progress(
    issues_stack: &RefCell<gtk4::glib::WeakRef<gtk4::Stack>>,
    progress_cards: &RefCell<Vec<gtk4::glib::WeakRef<gtk4::Widget>>>,
) {
    let progress_visible = progress_cards
        .borrow()
        .iter()
        .filter_map(gtk4::glib::WeakRef::upgrade)
        .any(|card| {
            card.downcast_ref::<gtk4::Revealer>().map_or_else(
                || card.is_visible(),
                |revealer| revealer.reveals_child() || revealer.is_child_revealed(),
            )
        });
    let stack = issues_stack.borrow().upgrade();
    if let Some(stack) = stack {
        super::sidebar_issues_section::show_issues_surface(
            &stack,
            super::sidebar_issues_section::issues_surface_for_progress(progress_visible),
        );
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
            Some(device.upcast_ref())
        );
        let spacer = slot
            .progress_widget()
            .first_child()
            .expect("progress root must reserve flexible space above its cards");
        assert!(spacer.is_visible());
        assert!(spacer.vexpands());
        assert_eq!(spacer.next_sibling().as_ref(), Some(scan.upcast_ref()));
        assert_eq!(
            slot.progress_widget().last_child().as_ref(),
            Some(relink.upcast_ref())
        );
        assert!(!scan.is_visible());
        assert!(!doctor.is_visible());
        assert!(!relink.is_visible());

        device.set_visible(true);
        scan.set_reveal_child(true);
        relink.set_reveal_child(true);
        doctor.set_reveal_child(true);
        assert!(device.is_visible());
        assert!(scan.reveals_child());
        assert!(relink.reveals_child());
        assert!(doctor.reveals_child());
        assert!(scan.is_visible());
        assert!(relink.is_visible());
        assert!(doctor.is_visible());
    }
}
