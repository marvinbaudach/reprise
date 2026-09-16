//! Diagnostic sidebar geometry report for the composed-window display tests.

use gtk4::prelude::*;
use libadwaita as adw;

use super::{descendants, WindowLayoutTestHandles};

fn format_bounds(bounds: Option<gtk4::graphene::Rect>) -> String {
    match bounds {
        Some(bounds) => format!(
            "(x={:.1}, y={:.1}, w={:.1}, h={:.1}, bottom={:.1})",
            bounds.x(),
            bounds.y(),
            bounds.width(),
            bounds.height(),
            bounds.y() + bounds.height(),
        ),
        None => "NONE".to_string(),
    }
}

fn widget_line(
    depth: usize,
    label: &str,
    widget: &gtk4::Widget,
    window: &adw::ApplicationWindow,
) -> String {
    let bounds = widget.compute_bounds(window);
    let (min, natural, _, _) = widget.measure(gtk4::Orientation::Vertical, widget.width());
    format!(
        "depth={depth} {label}: type={} classes={:?} bounds={} measure_v=(min={min}, nat={natural}) vexpand={} compute_expand_v={} valign={:?} visible={} child_visible={} mapped={}\n",
        widget.type_().name(),
        widget.css_classes(),
        format_bounds(bounds),
        widget.vexpands(),
        widget.compute_expand(gtk4::Orientation::Vertical),
        widget.valign(),
        widget.is_visible(),
        widget.is_child_visible(),
        widget.is_mapped(),
    )
}

/// Prints every descendant of `parent`, depth-first, down to `max_depth`.
/// The activity slot has bounded fan-out, unlike the navigation list.
fn dump_children(
    parent: &gtk4::Widget,
    depth: usize,
    max_depth: usize,
    window: &adw::ApplicationWindow,
    report: &mut String,
) {
    if depth > max_depth {
        return;
    }
    let mut child = parent.first_child();
    let mut index = 0usize;
    while let Some(current) = child {
        report.push_str(&widget_line(
            depth,
            &format!("activity slot child #{index}"),
            &current,
            window,
        ));
        dump_children(&current, depth + 1, max_depth, window, report);
        index += 1;
        child = current.next_sibling();
    }
}

/// Finds the visible mapped leaf with the largest bottom edge, optionally
/// capped at the sidebar page bottom so realized clipped rows cannot win.
fn deepest_leaf(
    root: &impl IsA<gtk4::Widget>,
    window: &adw::ApplicationWindow,
    max_bottom: Option<f32>,
) -> Option<(gtk4::Widget, f32)> {
    descendants(root)
        .into_iter()
        .filter(|widget| widget.first_child().is_none())
        .filter(|widget| widget.is_visible() && widget.is_mapped())
        .filter_map(|widget| {
            widget
                .compute_bounds(window)
                .map(|bounds| (widget, bounds.y() + bounds.height()))
        })
        .filter(|(_, bottom)| max_bottom.is_none_or(|max| *bottom <= max))
        .fold(
            None,
            |acc: Option<(gtk4::Widget, f32)>, (widget, bottom)| match &acc {
                Some((_, current)) if *current >= bottom => acc,
                _ => Some((widget, bottom)),
            },
        )
}

fn describe_leaf(leaf: &Option<(gtk4::Widget, f32)>) -> String {
    match leaf {
        Some((widget, bottom)) => format!(
            "type={} classes={:?} bottom={bottom:.1}",
            widget.type_().name(),
            widget.css_classes(),
        ),
        None => "NONE".to_string(),
    }
}

/// Reports the ancestor chain, curated sidebar subtree, player bar, and both
/// raw and page-clipped painted gaps for every geometry assertion.
pub(super) fn sidebar_report(handles: &WindowLayoutTestHandles) -> String {
    let window = &handles.window;
    let mut report = String::new();

    report.push_str("-- ancestor chain (top-down: window -> sidebar root box) --\n");
    let sidebar_widget: gtk4::Widget = handles.sidebar.widget().clone().upcast();
    let window_widget: gtk4::Widget = window.clone().upcast();
    let mut chain = vec![sidebar_widget.clone()];
    let mut node = sidebar_widget.parent();
    while let Some(current) = node {
        let reached_window = current == window_widget;
        chain.push(current.clone());
        if reached_window {
            break;
        }
        node = current.parent();
    }
    chain.reverse();
    for (depth, widget) in chain.iter().enumerate() {
        report.push_str(&widget_line(depth, "ancestor", widget, window));
    }

    report.push_str("-- sidebar subtree (curated) --\n");
    report.push_str(&widget_line(0, "root box", &sidebar_widget, window));

    let nav_scroller: gtk4::Widget = handles.navigation_scroller.clone().upcast();
    report.push_str(&widget_line(
        1,
        "navigation scroller",
        &nav_scroller,
        window,
    ));
    if let Some(nav_child) = nav_scroller.first_child() {
        report.push_str(&widget_line(
            2,
            "navigation scroller child (viewport)",
            &nav_child,
            window,
        ));
        if let Some(places) = nav_child.first_child() {
            report.push_str(&widget_line(3, "navigation places box", &places, window));
        }
    }

    let pinned: gtk4::Widget = handles.pinned_block.clone();
    report.push_str(&widget_line(1, "pinned scroller", &pinned, window));
    if let Some(viewport) = pinned.first_child() {
        report.push_str(&widget_line(2, "pinned viewport", &viewport, window));
        if let Some(region) = viewport.first_child() {
            report.push_str(&widget_line(3, "region box", &region, window));
            if let Some(issues_box) = region.first_child() {
                report.push_str(&widget_line(4, "issues box", &issues_box, window));
                if let Some(heading) = issues_box.first_child() {
                    report.push_str(&widget_line(5, "ISSUES heading", &heading, window));
                }
                let issues_listbox: gtk4::Widget = handles.issues_listbox.clone().upcast();
                report.push_str(&widget_line(5, "issues listbox", &issues_listbox, window));
                let mut row = handles.issues_listbox.first_child();
                let mut row_index = 0usize;
                while let Some(current) = row {
                    report.push_str(&widget_line(
                        6,
                        &format!("issue row #{row_index}"),
                        &current,
                        window,
                    ));
                    row_index += 1;
                    row = current.next_sibling();
                }
            }
            let activity: gtk4::Widget = handles.activity_slot.clone().upcast();
            report.push_str(&widget_line(
                4,
                "activity slot (progress_root)",
                &activity,
                window,
            ));
            dump_children(&activity, 5, 6, window, &mut report);
        }
    }

    report.push_str("-- player bar --\n");
    if let Some(bar) = &handles.player_bar {
        report.push_str(&widget_line(0, "player bar", bar, window));
    } else {
        report.push_str("player bar: NONE\n");
    }

    let sidebar_page: gtk4::Widget = handles.sidebar_page.clone().upcast();
    report.push_str(&format!(
        "sidebar_page bounds = {}\n",
        format_bounds(sidebar_page.compute_bounds(window)),
    ));

    let page_bottom = sidebar_page
        .compute_bounds(window)
        .map(|bounds| bounds.y() + bounds.height());
    let bar_y = handles
        .player_bar
        .as_ref()
        .and_then(|bar| bar.compute_bounds(window))
        .map(|bounds| bounds.y());
    let raw_leaf = deepest_leaf(&handles.sidebar_page, window, None);
    let painted_leaf = deepest_leaf(&handles.sidebar_page, window, page_bottom);
    for (label, leaf) in [("gap_raw", &raw_leaf), ("gap_painted", &painted_leaf)] {
        match (bar_y, leaf) {
            (Some(bar_y), Some((_, bottom))) => report.push_str(&format!(
                "{label} = {} (winner: {})\n",
                bar_y - bottom,
                describe_leaf(leaf)
            )),
            (bar_y, leaf) => report.push_str(&format!(
                "{label} = UNAVAILABLE (player_bar_y={bar_y:?}, winner={})\n",
                describe_leaf(leaf)
            )),
        }
    }
    report.push_str("gap = see gap_painted (the headline value); gap_raw flags a leaf painting below the sidebar page itself\n");
    report
}
