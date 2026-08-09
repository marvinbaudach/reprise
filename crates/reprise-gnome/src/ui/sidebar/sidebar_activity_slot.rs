//! Shared home for sidebar device state and replacement progress cards.
//!
//! Scan and device sync own their cards and update loops. This module owns
//! only their stable layout relationship, so construction order cannot move
//! either activity out of the slot or reorder the two relative to each other.

use std::cell::RefCell;

use gtk4::prelude::*;

pub(super) struct SidebarActivitySlot {
    /// Persistent device state remains independent from Issues/progress.
    root: gtk4::Box,
    /// Long-running cards, pinned below the Issues block (FB-8, amended).
    progress_root: gtk4::Box,
    progress_spacer: gtk4::Box,
    device_section: RefCell<Option<gtk4::Widget>>,
    scan_card: RefCell<Option<gtk4::Widget>>,
    doctor_card: RefCell<Option<gtk4::Widget>>,
    relink_card: RefCell<Option<gtk4::Widget>>,
}

impl SidebarActivitySlot {
    pub(super) fn new() -> Self {
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
        Self::track_progress_visibility(card);
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
        Self::track_progress_visibility(card);
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
        Self::track_progress_visibility(card);
    }

    /// An unrevealed `GtkRevealer` still reports its child's natural height, so
    /// the sidebar would reserve room for cards nobody can see. Keep each card's
    /// `visible` in step with its reveal state; that is all this tracking has to
    /// do now that the Issues block above it no longer moves out of the way
    /// (FB-8, amended).
    fn track_progress_visibility(card: &gtk4::Widget) {
        if let Some(revealer) = card.downcast_ref::<gtk4::Revealer>() {
            revealer.add_css_class("sidebar-job-card-dock");
            revealer.connect_reveal_child_notify(sync_revealer_visibility);
            revealer.connect_child_revealed_notify(sync_revealer_visibility);
            sync_revealer_visibility(revealer);
        }
    }
}

fn sync_revealer_visibility(revealer: &gtk4::Revealer) {
    let should_be_visible = revealer.reveals_child() || revealer.is_child_revealed();
    if revealer.is_visible() != should_be_visible {
        revealer.set_visible(should_be_visible);
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

    const JOB_CARD_HEIGHT_PX: f32 = 70.0;

    fn job_card(with_extra_action: bool) -> gtk4::Revealer {
        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 7);
        let spinner = gtk4::Spinner::new();
        spinner.add_css_class("scan-card-spinner");
        header.append(&spinner);
        let title = gtk4::Label::new(Some("Checking tracks…"));
        title.set_hexpand(true);
        title.add_css_class("scan-card-title");
        header.append(&title);
        let percent = gtk4::Label::new(Some("45%"));
        percent.add_css_class("scan-card-percent");
        header.append(&percent);
        if with_extra_action {
            let open = gtk4::Button::with_label("Open");
            open.add_css_class("flat");
            open.add_css_class("scan-card-compact-action");
            header.append(&open);
        }
        let cancel = gtk4::Button::with_label("Cancel");
        cancel.add_css_class("flat");
        cancel.add_css_class("scan-card-cancel");
        header.append(&cancel);
        let body = gtk4::Box::new(gtk4::Orientation::Vertical, 5);
        body.set_height_request(crate::ui::scan_card_css::JOB_CARD_HEIGHT_PX);
        body.add_css_class("scan-card");
        body.append(&header);
        body.append(&gtk4::ProgressBar::new());
        let detail = gtk4::Label::new(Some("742/1,648 tracks"));
        detail.add_css_class("scan-card-detail");
        body.append(&detail);
        gtk4::Revealer::builder()
            .transition_type(gtk4::RevealerTransitionType::None)
            .child(&body)
            .build()
    }

    fn pump() {
        while gtk4::glib::MainContext::default().iteration(false) {}
    }

    fn measured_job_card(with_extra_action: bool) -> gtk4::graphene::Rect {
        let slot = SidebarActivitySlot::new();
        let card = job_card(with_extra_action);
        slot.set_doctor_card(&card);
        let region = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        region.set_size_request(240, 470);
        region.append(slot.progress_widget());
        let window = gtk4::Window::builder()
            .default_width(240)
            .default_height(470)
            .child(&region)
            .build();
        card.set_reveal_child(true);
        window.present();
        pump();
        let bounds = card
            .child()
            .unwrap()
            .compute_bounds(&window)
            .expect("job card must be allocated");
        window.close();
        bounds
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn doc_5e_every_job_card_docks_at_the_same_place_and_height() {
        if gtk4::init().is_err() {
            return;
        }
        crate::ui::style::install_css_string_for_test(&crate::ui::scan_card_css::css());
        let doctor_bounds = measured_job_card(false);
        let relink_bounds = measured_job_card(true);

        assert_eq!(doctor_bounds.y(), relink_bounds.y());
        assert_eq!(doctor_bounds.height(), relink_bounds.height());
        assert_eq!(doctor_bounds.height(), JOB_CARD_HEIGHT_PX);
    }

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

        // One job is one card. Re-attaching — which happens whenever the
        // Doctor's coordinator is rebuilt — replaces the previous widget instead
        // of stacking a second one next to it.
        slot.set_doctor_card(&doctor);
        slot.set_doctor_card(&doctor);
        let mut cards = 0;
        let mut child = slot.progress_widget().first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if widget.downcast_ref::<gtk4::Revealer>().is_some() {
                cards += 1;
            }
        }
        assert_eq!(cards, 3, "scan, doctor and relink — one card each");

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
