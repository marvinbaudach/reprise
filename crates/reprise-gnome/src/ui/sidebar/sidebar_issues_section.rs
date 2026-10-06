//! Bottom-pinned region: the Issues block, and the progress cards under it.
//!
//! **FB-8, amended 2026-08-07.** Until this change a visible progress card
//! *replaced* the whole Issues block — heading, `Missing files`, import errors
//! and all — so starting any scan made the `ISSUES` section disappear, and the
//! Library Doctor's own result entry could not be seen while the job that
//! produced it was running. The design shows both at once: the sections stay,
//! and the cards sit at the very bottom beneath them. That is what this builds.

use gtk4::prelude::*;
use std::rc::Rc;

use super::sidebar_activity_slot::SidebarActivitySlot;
use super::sidebar_presentation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct BottomRegionPlacement {
    pub(super) vexpand: bool,
    pub(super) valign: gtk4::Align,
}

pub(super) fn bottom_region_placement() -> BottomRegionPlacement {
    BottomRegionPlacement {
        vexpand: false,
        valign: gtk4::Align::End,
    }
}

pub(super) fn build_issues_section(
    activity_slot: &SidebarActivitySlot,
    issues_listbox: &gtk4::ListBox,
) -> gtk4::Box {
    let issues = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let heading = sidebar_presentation::problem_header();
    issues_listbox
        .bind_property("visible", &heading, "visible")
        .sync_create()
        .build();
    issues.append(&heading);
    issues.append(issues_listbox);

    let placement = bottom_region_placement();
    let region = gtk4::Box::builder()
        .orientation(gtk4::Orientation::Vertical)
        .vexpand(placement.vexpand)
        .valign(placement.valign)
        .build();
    region.append(&issues);
    // The cards keep their own Revealer fade; the section above them no longer
    // moves out of the way, so nothing has to be switched atomically.
    region.append(activity_slot.progress_widget());
    region
}

pub(super) fn build_scrollable_issues_section(
    activity_slot: &SidebarActivitySlot,
    issues_listbox: &gtk4::ListBox,
) -> gtk4::ScrolledWindow {
    let region = build_issues_section(activity_slot, issues_listbox);
    let scrolled = gtk4::ScrolledWindow::builder()
        .child(&region)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .propagate_natural_height(true)
        .vexpand(false)
        .valign(gtk4::Align::End)
        .build();
    let sync_visibility: Rc<dyn Fn()> = Rc::new({
        let scrolled = scrolled.downgrade();
        let issues_listbox = issues_listbox.downgrade();
        let progress = activity_slot.progress_widget().downgrade();
        move || {
            if let (Some(scrolled), Some(issues_listbox), Some(progress)) = (
                scrolled.upgrade(),
                issues_listbox.upgrade(),
                progress.upgrade(),
            ) {
                scrolled.set_visible(
                    issues_listbox.property::<bool>("visible")
                        || progress_has_visible_card(&progress),
                );
            }
        }
    });
    issues_listbox.connect_visible_notify({
        let sync_visibility = sync_visibility.clone();
        move |_| sync_visibility()
    });
    sync_visibility();
    scrolled
}

pub(super) fn progress_has_visible_card(progress: &gtk4::Box) -> bool {
    std::iter::successors(
        progress.first_child(),
        gtk4::prelude::WidgetExt::next_sibling,
    )
    .filter_map(|child| child.downcast::<gtk4::Revealer>().ok())
    .any(|card| card.reveals_child() || card.is_child_revealed())
}
