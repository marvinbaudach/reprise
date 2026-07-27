//! Bottom-pinned region that shows either Issues or active scan progress.

use gtk4::prelude::*;

use super::sidebar_activity_slot::SidebarActivitySlot;
use super::sidebar_presentation;

const ISSUES_PAGE: &str = "issues";
const ACTIVITY_PAGE: &str = "activity";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum IssuesSurface {
    Issues,
    Activity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProgressPageChild {
    FlexibleSpace,
    Cards,
}

pub(super) fn progress_page_order() -> &'static [ProgressPageChild] {
    &[ProgressPageChild::FlexibleSpace, ProgressPageChild::Cards]
}

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

pub(super) fn issues_surface_for_progress(progress_visible: bool) -> IssuesSurface {
    if progress_visible {
        IssuesSurface::Activity
    } else {
        IssuesSurface::Issues
    }
}

pub(super) fn show_issues_surface(stack: &gtk4::Stack, surface: IssuesSurface) {
    let name = match surface {
        IssuesSurface::Issues => ISSUES_PAGE,
        IssuesSurface::Activity => ACTIVITY_PAGE,
    };
    stack.set_visible_child_name(name);
}

pub(super) fn build_issues_section(
    activity_slot: &SidebarActivitySlot,
    issues_listbox: &gtk4::ListBox,
) -> gtk4::Stack {
    let issues = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let heading = sidebar_presentation::problem_header();
    issues_listbox
        .bind_property("visible", &heading, "visible")
        .sync_create()
        .build();
    issues.append(&heading);
    issues.append(issues_listbox);

    let activity = activity_slot.progress_widget();
    activity.set_vexpand(false);
    activity.set_valign(gtk4::Align::End);
    let activity_page = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    // GtkStack allocates its active page at the page size even when the
    // child itself requests natural height. Keep any surplus allocation
    // above the cards explicitly; aligning the direct page child alone does
    // not move a vertical Box's own children away from its start edge.
    for child in progress_page_order() {
        match child {
            ProgressPageChild::FlexibleSpace => {
                let spacer = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
                spacer.set_vexpand(true);
                activity_page.append(&spacer);
            }
            ProgressPageChild::Cards => activity_page.append(activity),
        }
    }

    let placement = bottom_region_placement();
    let stack = gtk4::Stack::builder()
        .vexpand(placement.vexpand)
        .valign(placement.valign)
        .vhomogeneous(true)
        // The scanner's own Revealer owns its fade. Switch the surrounding
        // region atomically so the Issues heading and rows never remain
        // readable behind the card.
        .transition_type(gtk4::StackTransitionType::None)
        .build();
    stack.add_named(&issues, Some(ISSUES_PAGE));
    stack.add_named(&activity_page, Some(ACTIVITY_PAGE));
    show_issues_surface(&stack, IssuesSurface::Issues);
    activity_slot.attach_issues_stack(&stack);
    stack
}
