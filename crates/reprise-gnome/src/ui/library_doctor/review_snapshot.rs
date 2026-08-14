use std::collections::HashMap;

use reprise_core::library_doctor::{DoctorReviewRowId, DoctorReviewRowState};

use super::review_model::ReviewRowModel;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct AlbumCounts {
    pub(super) selected: usize,
    pub(super) selectable: usize,
    pub(super) changes: usize,
    pub(super) selectable_row_ids: Vec<DoctorReviewRowId>,
    pub(super) blocked_by: Option<DoctorReviewRowState>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ReviewTotals {
    pub(super) selected: usize,
    pub(super) selectable: usize,
    pub(super) changes: usize,
    pub(super) albums: usize,
}

#[derive(Debug, Clone, Default)]
pub(super) struct ReviewSnapshot {
    pub(super) rows: Vec<ReviewRowModel>,
    pub(super) albums: HashMap<String, AlbumCounts>,
    pub(super) totals: ReviewTotals,
}

impl ReviewSnapshot {
    pub(super) fn from_rows(rows: Vec<ReviewRowModel>) -> Self {
        let mut albums = HashMap::<String, AlbumCounts>::new();
        let mut totals = ReviewTotals::default();
        for row in &rows {
            let album = albums.entry(row.album_key.clone()).or_default();
            album.selected += row.selected_change_count;
            album.selectable += row.selectable_row_ids.len();
            album.changes += row.row_ids.len();
            album
                .selectable_row_ids
                .extend(row.selectable_row_ids.iter().copied());
            album.blocked_by = blocked_state(album.blocked_by, row.row.state);
            totals.selected += row.selected_change_count;
            totals.selectable += row.selectable_row_ids.len();
            totals.changes += row.selectable_row_ids.len();
        }
        totals.albums = albums.len();
        Self {
            rows,
            albums,
            totals,
        }
    }
}

fn blocked_state(
    current: Option<DoctorReviewRowState>,
    state: DoctorReviewRowState,
) -> Option<DoctorReviewRowState> {
    if current == Some(DoctorReviewRowState::Stale) || state == DoctorReviewRowState::Stale {
        Some(DoctorReviewRowState::Stale)
    } else if current == Some(DoctorReviewRowState::Conflict)
        || state == DoctorReviewRowState::Conflict
    {
        Some(DoctorReviewRowState::Conflict)
    } else {
        None
    }
}
