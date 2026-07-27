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

    let stack = gtk4::Stack::builder()
        .vhomogeneous(true)
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
