//! Vertical layout for the navigation, activity, and issue collections.

use gtk4::prelude::*;

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
    activity_slot: &gtk4::Box,
    issues_listbox: &gtk4::ListBox,
) -> gtk4::Box {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    for child in sidebar_root_order() {
        match child {
            SidebarRootChild::Navigation => root.append(scrolled),
            SidebarRootChild::Activity => root.append(activity_slot),
            SidebarRootChild::Issues => root.append(issues_listbox),
        }
    }
    root
}
