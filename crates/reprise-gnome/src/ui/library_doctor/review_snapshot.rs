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
    #[cfg_attr(not(test), allow(dead_code))]
    index: HashMap<DoctorReviewRowId, u32>,
    pub(super) totals: ReviewTotals,
}

impl ReviewSnapshot {
    pub(super) fn from_rows(rows: Vec<ReviewRowModel>) -> Self {
        let mut albums = HashMap::<String, AlbumCounts>::new();
        let mut index = HashMap::new();
        let mut totals = ReviewTotals::default();
        for (position, row) in rows.iter().enumerate() {
            let position = u32::try_from(position).expect("review row count fits u32");
            for row_id in &row.row_ids {
                debug_assert!(
                    index.insert(*row_id, position).is_none(),
                    "a review row id belongs to exactly one display row"
                );
            }
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
            index,
            totals,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn selection_diff(
        &self,
        session: &reprise_core::library_doctor::DoctorReviewSession,
    ) -> Vec<(u32, ReviewRowModel)> {
        let rows_by_id = session
            .rows()
            .iter()
            .map(|row| (row.id, row))
            .collect::<HashMap<_, _>>();
        self.rows
            .iter()
            .filter_map(|cached| {
                let selected_change_count = cached
                    .row_ids
                    .iter()
                    .filter_map(|id| rows_by_id.get(id))
                    .filter(|row| row.selected && row.state == DoctorReviewRowState::Ready)
                    .count();
                let selected = !cached.selectable_row_ids.is_empty()
                    && selected_change_count == cached.selectable_row_ids.len();
                if selected_change_count == cached.selected_change_count
                    && selected == cached.row.selected
                {
                    return None;
                }
                let mut changed = cached.clone();
                changed.selected_change_count = selected_change_count;
                changed.row.selected = selected;
                let position = cached
                    .row_ids
                    .first()
                    .and_then(|row_id| self.index.get(row_id))
                    .copied()
                    .expect("every cached review row has an indexed row id");
                Some((position, changed))
            })
            .collect()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn with_selection(mut self, changed: &[(u32, ReviewRowModel)]) -> Self {
        for (position, replacement) in changed {
            let position = usize::try_from(*position).expect("review row position fits usize");
            let cached = self
                .rows
                .get(position)
                .expect("selection diff points inside the cached review rows");
            debug_assert_eq!(cached.album_key, replacement.album_key);
            let album = self
                .albums
                .get_mut(&cached.album_key)
                .expect("every cached review row belongs to a cached album");
            album.selected = album
                .selected
                .checked_sub(cached.selected_change_count)
                .expect("cached album selection includes its row")
                + replacement.selected_change_count;
            self.totals.selected = self
                .totals
                .selected
                .checked_sub(cached.selected_change_count)
                .expect("cached page selection includes its row")
                + replacement.selected_change_count;
            self.rows[position] = replacement.clone();
        }
        self
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
