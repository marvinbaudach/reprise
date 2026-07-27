//! Bottom-pinned Issues section layout: heading, activity, then issue sources.

use gtk4::prelude::*;

use super::sidebar_presentation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum IssuesSectionChild {
    Heading,
    Activity,
    Sources,
}

pub(super) fn issues_section_order() -> [IssuesSectionChild; 3] {
    [
        IssuesSectionChild::Heading,
        IssuesSectionChild::Activity,
        IssuesSectionChild::Sources,
    ]
}

pub(super) fn issues_activity_alignment() -> gtk4::Align {
    gtk4::Align::End
}

pub(super) fn build_issues_section(
    activity_slot: &gtk4::Box,
    issues_listbox: &gtk4::ListBox,
) -> gtk4::Box {
    let section = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    // Some activity children (notably revealers around progress cards) can
    // propagate vertical expansion through their slot. Explicitly suppress
    // that computed expansion and bottom-align any defensive extra
    // allocation, so no flexible gap can open between a running card and
    // Import errors / Missing files.
    activity_slot.set_vexpand(false);
    activity_slot.set_valign(issues_activity_alignment());
    let heading = sidebar_presentation::problem_header();
    issues_listbox
        .bind_property("visible", &heading, "visible")
        .sync_create()
        .build();
    for child in issues_section_order() {
        match child {
            IssuesSectionChild::Heading => section.append(&heading),
            IssuesSectionChild::Activity => section.append(activity_slot),
            IssuesSectionChild::Sources => section.append(issues_listbox),
        }
    }
    section
}
