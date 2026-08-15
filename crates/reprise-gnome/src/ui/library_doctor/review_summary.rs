//! What the review page *states* about the rows it is showing: the header's
//! inventory, the stale notice above the list, and the footer's summary.
//!
//! These are derivations from the visible row set and nothing else — no widget,
//! no `ReviewState`. They live apart from `review_page.rs` so that page keeps
//! the wiring and this file keeps the arithmetic.

use reprise_core::library_doctor::{DoctorReviewRowState, DoctorReviewSession};

use super::review_model::ReviewCategory;
#[cfg(test)]
use super::review_model::ReviewRowModel;
#[cfg(test)]
use super::review_snapshot::ReviewSnapshot;
use crate::ui::strings;

/// What the header states: the changes on screen and the albums they sit in.
///
/// This is the inventory, not the selection — the (possibly filtered) rows the
/// page is showing. Unchecking a row leaves it standing, which is the whole
/// point of the split: the header answers "what is here", the footer and the
/// Apply button answer "what will be written". Mixing the two produced
/// "1 changes · 2 albums", where the first number followed the checkbox and the
/// second did not.
#[cfg(test)]
pub(super) fn review_header_counts(rows: &[ReviewRowModel]) -> (usize, usize) {
    let totals = ReviewSnapshot::from_rows(rows.to_vec()).totals;
    (totals.changes, totals.albums)
}

pub(super) fn review_stale_notice(session: &DoctorReviewSession) -> Option<String> {
    let count = session
        .rows()
        .iter()
        .filter(|row| session.category_filter_matches(row.problem_class))
        .filter(|row| row.state == DoctorReviewRowState::Stale)
        .count();
    (count > 0).then(|| strings::doctor_stale_notice(count))
}

pub(super) fn review_footer_summary(
    summary: reprise_core::library_doctor::DoctorReviewSummary,
    category: Option<ReviewCategory>,
    ready_count: usize,
) -> String {
    category.map_or_else(
        || strings::doctor_apply_summary(summary.tag_change_count, ready_count, summary.file_count),
        |category| {
            strings::doctor_filter_scope(
                summary.tag_change_count,
                summary.total_tag_change_count,
                &strings::text(category.label()),
            )
        },
    )
}
