//! "Recent transfers" on the device page: what each run did (MTP-20).
//!
//! The core records runs but deliberately holds no wording, so the phrasing
//! lives here. A run is one expandable row — headline and balance closed,
//! its deviations inside. Successful copies are a number, not a list.

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::device_sync::sync_log::{Deviation, DeviationKind, RunOutcome, RunRecord};

/// How many runs the page offers before it stops being scannable.
pub(super) const SHOWN_RUNS: usize = 10;

/// One recorded run with the deviations that belong to it.
pub(super) type RunWithDeviations = (RunRecord, Vec<Deviation>);

/// Replaces the card inside `container` with one built from `runs`.
pub(super) fn fill(container: &gtk4::Box, runs: &[RunWithDeviations]) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
    container.append(&build(runs));
}

fn build(runs: &[RunWithDeviations]) -> gtk4::Box {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.add_css_class("reprise-device-history");

    let title = gtk4::Label::builder()
        .label("Recent transfers")
        .xalign(0.0)
        .build();
    title.add_css_class("title-2");
    content.append(&title);

    if runs.is_empty() {
        let empty = gtk4::Label::builder()
            .label("No synchronization has run yet.")
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
        let row = adw::ExpanderRow::builder()
            .title(run_headline(run))
            .subtitle(run_balance(run))
            .build();
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

/// When it ran and how it ended.
fn run_headline(run: &RunRecord) -> String {
    let when = chrono::DateTime::from_timestamp(run.started_at, 0).map_or_else(
        || "unknown time".to_owned(),
        |stamp| {
            chrono::DateTime::<chrono::Local>::from(stamp)
                .format("%-d %b %Y, %H:%M")
                .to_string()
        },
    );
    format!("{when} · {}", outcome_word(run.outcome))
}

fn outcome_word(outcome: RunOutcome) -> &'static str {
    match outcome {
        RunOutcome::Running => "Running",
        RunOutcome::Completed => "Completed",
        RunOutcome::Cancelled => "Cancelled",
        RunOutcome::Failed => "Failed",
        RunOutcome::Interrupted => "Interrupted",
    }
}

/// What arrived and what did not. Zero counts are left out — a clean run
/// should read as clean, not as a row of noughts.
fn run_balance(run: &RunRecord) -> String {
    let mut parts = vec![format!("{} of {} copied", run.copied, run.planned)];
    if run.skipped > 0 {
        parts.push(format!("{} skipped", run.skipped));
    }
    if run.failed > 0 {
        parts.push(format!("{} failed", run.failed));
    }
    if run.deleted > 0 {
        parts.push(format!("{} removed", run.deleted));
    }
    if let Some(detail) = &run.detail {
        parts.push(detail.clone());
    }
    parts.join(" · ")
}

/// One file that did not go through cleanly.
fn deviation_line(deviation: &Deviation) -> String {
    format!(
        "{} · {} — {}",
        kind_word(deviation.kind),
        deviation.device_path,
        deviation.detail
    )
}

fn kind_word(kind: DeviationKind) -> &'static str {
    match kind {
        DeviationKind::Skipped => "Skipped",
        DeviationKind::Failed => "Failed",
        DeviationKind::Deleted => "Removed",
        DeviationKind::ConversionFallback => "Kept original",
        DeviationKind::PlaylistWriteFailed => "Playlist failed",
    }
}

#[cfg(test)]
#[path = "device_sync_history_tests.rs"]
mod tests;
