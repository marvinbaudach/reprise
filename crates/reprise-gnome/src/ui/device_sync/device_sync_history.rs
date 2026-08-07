//! "Recent syncs" on the device page: what each run did (MTP-20).
//!
//! The core records runs but deliberately holds no wording, so the phrasing
//! lives in the GTK layer's `device_sync_strings` sibling. A run is one
//! expandable row — headline and balance closed, its deviations inside.
//! Successful copies are a number, not a list.

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::device_sync::sync_log::{Deviation, RunOutcome, RunRecord};

use super::device_sync_strings;

/// How many runs the page offers before it stops being scannable.
pub(super) const SHOWN_RUNS: usize = 10;

/// One recorded run with the deviations that belong to it.
pub(super) type RunWithDeviations = (RunRecord, Vec<Deviation>);

#[derive(Clone, Debug, PartialEq)]
pub(super) struct RunningProgress {
    pub(super) title: String,
    pub(super) fraction: f64,
}

#[derive(Debug, PartialEq)]
struct RunRowCopy {
    headline: String,
    subtitle: String,
    percent: Option<u64>,
}

/// Replaces the card inside `container` with one built from `runs`.
pub(super) fn fill(
    container: &gtk4::Box,
    runs: &[RunWithDeviations],
    rememberable: bool,
    running_progress: Option<&RunningProgress>,
) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
    container.append(&build(runs, rememberable, running_progress));
}

fn build(
    runs: &[RunWithDeviations],
    rememberable: bool,
    running_progress: Option<&RunningProgress>,
) -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 9);
    content.add_css_class("reprise-device-history");

    let title = gtk4::Label::builder()
        .label(device_sync_strings::sync_history_heading())
        .xalign(0.0)
        .build();
    title.add_css_class("title-2");
    let caption = gtk4::Label::builder()
        .label(device_sync_strings::sync_history_caption(SHOWN_RUNS))
        .xalign(1.0)
        .hexpand(true)
        .build();
    caption.add_css_class("dim-label");
    let heading = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    heading.append(&title);
    heading.append(&caption);
    content.append(&heading);

    if let Some((title, detail)) = history_warning_copy(rememberable) {
        let banner = gtk4::Box::new(gtk4::Orientation::Vertical, 3);
        banner.add_css_class("warning");
        banner.set_margin_top(3);
        banner.set_margin_bottom(3);
        banner.set_margin_start(12);
        banner.set_margin_end(12);
        let title = gtk4::Label::builder()
            .label(title)
            .xalign(0.0)
            .wrap(true)
            .build();
        title.add_css_class("heading");
        let detail = gtk4::Label::builder()
            .label(detail)
            .xalign(0.0)
            .wrap(true)
            .build();
        detail.add_css_class("dim-label");
        banner.append(&title);
        banner.append(&detail);
        content.append(&banner);
    }

    if runs.is_empty() {
        let empty = gtk4::Label::builder()
            .label(device_sync_strings::sync_history_empty_state())
            .xalign(0.0)
            .build();
        empty.add_css_class("dim-label");
        content.append(&empty);
        return content;
    }

    let list = gtk4::ListBox::new();
    list.set_selection_mode(gtk4::SelectionMode::None);
    list.add_css_class("boxed-list");
    for (run, found) in runs.iter().take(SHOWN_RUNS) {
        let live = (run.outcome == RunOutcome::Running)
            .then_some(running_progress)
            .flatten();
        let copy = run_row_copy(run, live);
        let row = adw::ExpanderRow::builder()
            .title(copy.headline)
            .subtitle(copy.subtitle)
            .build();
        let state = device_sync_strings::sync_history_state(run.outcome);
        let icon = gtk4::Image::from_icon_name(state.icon);
        icon.add_css_class(state.colour);
        row.add_prefix(&icon);
        if let Some(percent) = copy.percent {
            let percent = gtk4::Label::new(Some(&format!("{percent} %")));
            percent.add_css_class("dim-label");
            row.add_suffix(&percent);
        }
        if found.is_empty() {
            row.set_enable_expansion(false);
        }
        for deviation in found {
            let detail = adw::ActionRow::builder()
                .title(deviation_line(deviation))
                .css_classes(["dim-label"])
                .build();
            detail.set_title_lines(2);
            row.add_row(&detail);
        }
        list.append(&row);
    }
    content.append(&list);
    content
}

fn history_warning_copy(rememberable: bool) -> Option<(String, String)> {
    (!rememberable).then(device_sync_strings::sync_history_unrecorded_warning)
}

fn run_row_copy(run: &RunRecord, running_progress: Option<&RunningProgress>) -> RunRowCopy {
    if run.outcome == RunOutcome::Running {
        let headline = chrono::DateTime::from_timestamp(run.started_at, 0).map_or_else(
            || {
                device_sync_strings::sync_history_running_since(
                    &device_sync_strings::sync_history_unknown_time(),
                )
            },
            |stamp| {
                let time = chrono::DateTime::<chrono::Local>::from(stamp)
                    .format("%H:%M")
                    .to_string();
                device_sync_strings::sync_history_running_since(&time)
            },
        );
        return RunRowCopy {
            headline,
            subtitle: running_progress
                .map_or_else(device_sync_strings::sync_history_running, |copy| {
                    copy.title.clone()
                }),
            percent: running_progress
                .map(|copy| (copy.fraction.clamp(0.0, 1.0) * 100.0).round() as u64),
        };
    }
    RunRowCopy {
        headline: run_headline(run),
        subtitle: run_balance(run),
        percent: None,
    }
}

/// When it ran and how it ended.
fn run_headline(run: &RunRecord) -> String {
    let when = chrono::DateTime::from_timestamp(run.started_at, 0).map_or_else(
        device_sync_strings::sync_history_unknown_time,
        |stamp| {
            chrono::DateTime::<chrono::Local>::from(stamp)
                .format("%-d %b %Y, %H:%M")
                .to_string()
        },
    );
    device_sync_strings::sync_history_headline(&when, run.outcome)
}

/// What arrived and what did not. Zero counts are left out — a clean run
/// should read as clean, not as a row of noughts.
fn run_balance(run: &RunRecord) -> String {
    device_sync_strings::sync_history_balance(run)
}

/// One file that did not go through cleanly.
fn deviation_line(deviation: &Deviation) -> String {
    device_sync_strings::sync_history_deviation_line(deviation)
}

#[cfg(test)]
#[path = "device_sync_history_tests.rs"]
mod tests;
