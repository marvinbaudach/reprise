//! Vertical layout for the navigation, activity, and issue collections.

use gtk4::prelude::*;

use super::sidebar_activity_slot::SidebarActivitySlot;
use super::sidebar_issues_section::build_issues_section;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SidebarRootChild {
    Navigation,
    Activity,
    Issues,
}

pub(super) fn sidebar_root_order() -> [SidebarRootChild; 3] {
    [
        SidebarRootChild::Navigation,
        SidebarRootChild::Activity,
        SidebarRootChild::Issues,
    ]
}

/// Assembles the scrolling navigation above the transient activity slot and
/// keeps the issues list pinned to the bottom edge (FB-2a).
pub(super) fn build_root(
    scrolled: &gtk4::ScrolledWindow,
    activity_slot: &SidebarActivitySlot,
    issues_listbox: &gtk4::ListBox,
) -> gtk4::Box {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    root.set_vexpand(true);
    let issues_section = build_issues_section(activity_slot, issues_listbox);
    for child in sidebar_root_order() {
        match child {
            SidebarRootChild::Navigation => root.append(scrolled),
            SidebarRootChild::Activity => root.append(activity_slot.widget()),
            SidebarRootChild::Issues => root.append(&issues_section),
        }
    }
    root
}
