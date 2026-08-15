use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::db::Db;
use reprise_core::library_doctor::{
    DoctorReviewGroup, DoctorReviewGroupId, DoctorUnresolvedGroup, DoctorValue, LibraryDoctor,
};

use super::review_model::{candidate_description, candidate_label, field_label};
use crate::ui::strings;

type OnChoose = Rc<dyn Fn(DoctorReviewGroupId, &DoctorValue)>;

type ConflictFingerprint = Vec<(DoctorReviewGroupId, Option<DoctorValue>)>;

pub(super) fn acknowledge_skipped_scan(db: &Db, scan_id: i64) -> Result<(), String> {
    LibraryDoctor::new(db)
        .set_reviewed_scan(scan_id)
        .map_err(|error| error.to_string())
}

#[derive(Default)]
pub(super) struct ReviewConflictsSlot {
    fingerprint: RefCell<ConflictFingerprint>,
    index: Cell<Option<u32>>,
}

impl ReviewConflictsSlot {
    pub(super) fn fingerprint(groups: &[DoctorReviewGroup]) -> ConflictFingerprint {
        groups
            .iter()
            .map(|group| (group.id, group.chosen.clone()))
            .collect()
    }

    pub(super) fn is_current(&self, fingerprint: &ConflictFingerprint) -> bool {
        self.index.get().is_some() && *self.fingerprint.borrow() == *fingerprint
    }

    pub(super) fn index(&self) -> Option<u32> {
        self.index.get()
    }

    pub(super) fn remember(&self, fingerprint: ConflictFingerprint, index: u32) {
        self.fingerprint.replace(fingerprint);
        self.index.set(Some(index));
    }

    pub(super) fn relocate(&self, index: u32) {
        if self.index.get().is_some() {
            self.index.set(Some(index));
        }
    }

    pub(super) fn clear(&self) -> Option<u32> {
        self.fingerprint.borrow_mut().clear();
        self.index.take()
    }
}

pub(super) struct ReviewConflicts {
    pub(super) root: gtk4::Box,
    pub(super) skip: gtk4::Button,
}

impl ReviewConflicts {
    pub(super) fn new(
        groups: &[DoctorReviewGroup],
        unresolved: &[DoctorUnresolvedGroup],
        on_choose: &OnChoose,
    ) -> Self {
        let title = gtk4::Label::builder()
            .label(strings::text(strings::DOCTOR_CONFLICTS_SECTION))
            .xalign(0.0)
            .css_classes(["heading"])
            .build();
        let optional = gtk4::Label::builder()
            .label(strings::text(strings::DOCTOR_CONFLICTS_OPTIONAL))
            .xalign(0.0)
            .css_classes(["doctor-conflicts-optional"])
            .build();
        let warning = gtk4::Image::builder()
            .icon_name("dialog-warning-symbolic")
            .pixel_size(17)
            .css_classes(["doctor-conflicts-warning"])
            .build();
        let skip = gtk4::Button::builder()
            .label(strings::text(strings::DOCTOR_SKIP_ALL))
            .css_classes(["flat"])
            .build();
        let heading = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        heading.append(&warning);
        heading.append(&title);
        heading.append(&optional);
        let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        heading.append(&spacer);
        heading.append(&skip);
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        root.set_margin_start(28);
        root.set_margin_end(28);
        root.set_margin_top(16);
        root.add_css_class("doctor-conflicts-dashed");
        root.append(&heading);
        let intro = gtk4::Label::builder()
            .label(strings::doctor_conflicts_intro(groups.len()))
            .xalign(0.0)
            .wrap(true)
            .css_classes(["doctor-conflicts-intro"])
            .build();
        intro.update_property(&[gtk4::accessible::Property::Description(&strings::text(
            strings::DOCTOR_PICK_ONE,
        ))]);
        let intro_clamp = libadwaita::Clamp::builder()
            .maximum_size(640)
            .tightening_threshold(640)
            .halign(gtk4::Align::Start)
            .child(&intro)
            .build();
        root.append(&intro_clamp);
        let member_counts = unresolved
            .iter()
            .map(|group| ((group.field, group.group_key.as_str()), group.members.len()))
            .collect::<HashMap<_, _>>();
        for group in groups {
            let count = member_counts
                .get(&(group.field, group.group_key.as_str()))
                .copied()
                .unwrap_or_default();
            let scope = gtk4::Label::builder()
                .label(strings::doctor_conflict_scope(
                    &strings::text(field_label(group.field)),
                    count,
                ))
                .xalign(0.0)
                .width_request(170)
                .css_classes(["doctor-conflict-scope"])
                .build();
            let choices = gtk4::FlowBox::builder()
                .column_spacing(6)
                .row_spacing(6)
                .selection_mode(gtk4::SelectionMode::None)
                .build();
            let mut first = None::<gtk4::ToggleButton>;
            for candidate in &group.candidates {
                let button = gtk4::ToggleButton::builder()
                    .label(candidate_label(candidate))
                    .tooltip_text(candidate_description(candidate))
                    .css_classes(["doctor-conflict-choice"])
                    .build();
                if let Some(first) = &first {
                    button.set_group(Some(first));
                } else {
                    first = Some(button.clone());
                }
                button.set_active(group.chosen.as_ref() == Some(&candidate.value));
                if button.is_active() {
                    button.add_css_class("selected");
                }
                button.update_property(&[gtk4::accessible::Property::Description(
                    &candidate_description(candidate),
                )]);
                let callback = on_choose.clone();
                let group_id = group.id;
                let value = candidate.value.clone();
                button.connect_toggled(move |button| {
                    if button.is_active() {
                        button.add_css_class("selected");
                    } else {
                        button.remove_css_class("selected");
                    }
                    if button.is_active() {
                        callback(group_id, &value);
                    }
                });
                choices.insert(&button, -1);
            }
            let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 16);
            row.add_css_class("doctor-conflict-row");
            row.append(&scope);
            row.append(&choices);
            root.append(&row);
        }
        root.set_visible(!groups.is_empty());
        Self { root, skip }
    }
}
