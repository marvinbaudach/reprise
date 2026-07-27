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

pub(super) fn build_issues_section(
    activity_slot: &gtk4::Box,
    issues_listbox: &gtk4::ListBox,
) -> gtk4::Box {
    let section = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
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
