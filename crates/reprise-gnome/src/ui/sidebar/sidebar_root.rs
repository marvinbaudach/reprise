//! Vertical layout for the navigation, activity, and issue collections.

use gtk4::prelude::*;

use super::sidebar_activity_slot::SidebarActivitySlot;
use super::sidebar_issues_section::{build_issues_section, build_scrollable_issues_section};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SidebarRootChild {
    Navigation,
    Issues,
}

pub(super) fn sidebar_root_order() -> [SidebarRootChild; 2] {
    [SidebarRootChild::Navigation, SidebarRootChild::Issues]
}

/// Assembles the scrolling navigation above the issues and running-job region.
/// Resting device state is already part of `scrolled` (NAV-20).
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
            SidebarRootChild::Issues => root.append(&issues_section),
        }
    }
    root
}

/// Production assembly: the pinned region yields inside its own viewport.
/// Small component fixtures retain the unwrapped region so their structural
/// assertions keep describing the components they were written to exercise.
pub(super) fn build_production_root(
    scrolled: &gtk4::ScrolledWindow,
    activity_slot: &SidebarActivitySlot,
    issues_listbox: &gtk4::ListBox,
) -> gtk4::Box {
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    root.set_vexpand(true);
    root.append(scrolled);
    root.append(&build_scrollable_issues_section(
        activity_slot,
        issues_listbox,
    ));
    root
}
