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
pub(super) struct BottomRegionPlacement {
    pub(super) vexpand: bool,
    pub(super) valign: gtk4::Align,
    pub(super) vhomogeneous: bool,
}

pub(super) fn bottom_region_placement() -> BottomRegionPlacement {
    BottomRegionPlacement {
        vexpand: false,
        valign: gtk4::Align::End,
        vhomogeneous: false,
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
    // The outer stack is explicitly non-expanding in the sidebar Box. Its
    // active page still needs to accept the stack's allocation so the page's
    // own spacer can place every visible card at the bottom.
    activity.set_vexpand(true);
    // GtkStack may allocate this page above its natural height. Fill that
    // allocation so progress_root's own spacer can keep the cards at the
    // bottom; Align::End leaves the Box's children at the allocation start.
    activity.set_valign(gtk4::Align::Fill);

    let placement = bottom_region_placement();
    let stack = gtk4::Stack::builder()
        .vexpand(placement.vexpand)
        .valign(placement.valign)
        .vhomogeneous(placement.vhomogeneous)
        // The scanner's own Revealer owns its fade. Switch the surrounding
        // region atomically so the Issues heading and rows never remain
        // readable behind the card.
        .transition_type(gtk4::StackTransitionType::None)
        .build();
    stack.add_named(&issues, Some(ISSUES_PAGE));
    stack.add_named(activity, Some(ACTIVITY_PAGE));
    show_issues_surface(&stack, IssuesSurface::Issues);
    activity_slot.attach_issues_stack(&stack);
    stack
}
