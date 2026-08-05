use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::library_doctor::{
    DoctorReviewGroup, DoctorReviewGroupId, DoctorUnresolvedGroup, DoctorValue,
};

use super::review_model::{candidate_description, field_label};
use crate::ui::strings;

type OnChoose = Rc<dyn Fn(DoctorReviewGroupId, &DoctorValue)>;

thread_local! {
    static STYLE_INSTALLED: Cell<bool> = const { Cell::new(false) };
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
            .css_classes(["caption", "dim-label"])
            .build();
        let skip = gtk4::Button::builder()
            .label(strings::text(strings::DOCTOR_SKIP_ALL))
            .css_classes(["flat"])
            .build();
        let heading = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        title.set_hexpand(true);
        heading.append(&title);
        heading.append(&skip);
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        root.set_margin_start(18);
        root.set_margin_end(18);
        root.set_margin_top(12);
        root.set_margin_bottom(12);
        root.add_css_class("card");
        root.add_css_class("doctor-conflicts-dashed");
        ensure_style(&root.display());
        root.append(&heading);
        root.append(&optional);
        root.append(
            &gtk4::Label::builder()
                .label(strings::text(strings::DOCTOR_PICK_ONE))
                .xalign(0.0)
                .build(),
        );
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
                .hexpand(true)
                .build();
            let choices = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
            let mut first = None::<gtk4::CheckButton>;
            for candidate in &group.candidates {
                let button = gtk4::CheckButton::builder()
                    .label(candidate_description(candidate))
                    .css_classes(["pill"])
                    .build();
                if let Some(first) = &first {
                    button.set_group(Some(first));
                } else {
                    first = Some(button.clone());
                }
                button.set_active(group.chosen.as_ref() == Some(&candidate.value));
                button.update_property(&[gtk4::accessible::Property::Description(
                    &candidate_description(candidate),
                )]);
                let callback = on_choose.clone();
                let group_id = group.id;
                let value = candidate.value.clone();
                button.connect_toggled(move |button| {
                    if button.is_active() {
                        callback(group_id, &value);
                    }
                });
                choices.append(&button);
            }
            let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
            row.append(&scope);
            row.append(&choices);
            root.append(&row);
        }
        root.set_visible(!groups.is_empty());
        Self { root, skip }
    }
}

fn ensure_style(display: &gtk4::gdk::Display) {
    STYLE_INSTALLED.with(|installed| {
        if installed.replace(true) {
            return;
        }
        let provider = gtk4::CssProvider::new();
        provider.load_from_string(
            ".doctor-conflicts-dashed { border: 1px dashed @borders; border-radius: 12px; padding: 12px; }",
        );
        gtk4::style_context_add_provider_for_display(
            display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });
}
