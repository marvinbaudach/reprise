use std::collections::{HashMap, HashSet};

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::library_doctor::{DoctorReviewRowId, DoctorReviewRowState, DoctorReviewSession};
use reprise_view::search_scope::matches_any;

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
    index: HashMap<DoctorReviewRowId, u32>,
    pub(super) totals: ReviewTotals,
    visible: Vec<bool>,
    pub(super) unfiltered_changes: usize,
}

impl ReviewSnapshot {
    pub(super) fn from_rows(rows: Vec<ReviewRowModel>, query: &str) -> Self {
        let mut index = HashMap::new();
        for (position, row) in rows.iter().enumerate() {
            let position = u32::try_from(position).expect("review row count fits u32");
            for row_id in &row.row_ids {
                if let Some(first_position) = index.get(row_id).copied() {
                    tracing::error!(
                        ?row_id,
                        first_position,
                        duplicate_position = position,
                        "Library Doctor review row id belongs to multiple display rows"
                    );
                } else {
                    index.insert(*row_id, position);
                }
            }
        }
        let unfiltered_changes = rows.iter().map(|row| row.selectable_row_ids.len()).sum();
        let mut snapshot = Self {
            visible: vec![false; rows.len()],
            rows,
            albums: HashMap::new(),
            index,
            totals: ReviewTotals::default(),
            unfiltered_changes,
        };
        snapshot.apply_query(query);
        snapshot
    }

    pub(super) fn apply_query(&mut self, query: &str) {
        for (visible, row) in self.visible.iter_mut().zip(&self.rows) {
            *visible = matches_any(
                [
                    row.track.as_str(),
                    row.album_title.as_str(),
                    row.album_artist.as_str(),
                ],
                query,
            );
        }
        self.albums.clear();
        self.totals = ReviewTotals::default();
        for row in self
            .rows
            .iter()
            .zip(&self.visible)
            .filter_map(|(row, visible)| visible.then_some(row))
        {
            let album = self.albums.entry(row.album_key.clone()).or_default();
            album.selected += row.selected_change_count;
            album.selectable += row.selectable_row_ids.len();
            album.changes += row.row_ids.len();
            album
                .selectable_row_ids
                .extend(row.selectable_row_ids.iter().copied());
            album.blocked_by = blocked_state(album.blocked_by, row.row.state);
            self.totals.selected += row.selected_change_count;
            self.totals.selectable += row.selectable_row_ids.len();
            self.totals.changes += row.selectable_row_ids.len();
        }
        self.totals.albums = self.albums.len();
    }

    pub(super) fn is_visible(&self, id: Option<&DoctorReviewRowId>) -> bool {
        id.and_then(|id| self.index.get(id))
            .and_then(|position| usize::try_from(*position).ok())
            .and_then(|position| self.visible.get(position))
            .copied()
            .unwrap_or(true)
    }

    pub(super) fn visible_selectable_row_ids(&self) -> HashSet<DoctorReviewRowId> {
        self.rows
            .iter()
            .zip(&self.visible)
            .filter(|(_, visible)| **visible)
            .flat_map(|(row, _)| row.selectable_row_ids.iter().copied())
            .collect()
    }

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

    pub(super) fn with_selection(mut self, changed: &[(u32, ReviewRowModel)]) -> Self {
        for (position, replacement) in changed {
            let position = usize::try_from(*position).expect("review row position fits usize");
            let cached = self
                .rows
                .get(position)
                .expect("selection diff points inside the cached review rows");
            debug_assert_eq!(cached.album_key, replacement.album_key);
            if !self.visible[position] {
                self.rows[position] = replacement.clone();
                continue;
            }
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

pub(super) fn splice_selection_rows(
    store: &gio::ListStore,
    changed: &[(u32, ReviewRowModel)],
    row_count: usize,
) {
    debug_assert_store_layout(store, row_count);
    debug_assert!(changed.windows(2).all(|pair| pair[0].0 < pair[1].0));
    let row_count = u32::try_from(row_count).expect("review row count fits u32");
    let mut run_start = 0;
    while run_start < changed.len() {
        let first_position = changed[run_start].0;
        let mut run_end = run_start + 1;
        while run_end < changed.len()
            && changed[run_end].0
                == first_position + u32::try_from(run_end - run_start).expect("run length fits u32")
        {
            run_end += 1;
        }
        debug_assert!(changed[run_start..run_end]
            .iter()
            .all(|(position, _)| *position < row_count));
        let objects = changed[run_start..run_end]
            .iter()
            .map(|(_, row)| glib::BoxedAnyObject::new(row.clone()).upcast::<glib::Object>())
            .collect::<Vec<_>>();
        store.splice(
            first_position,
            u32::try_from(objects.len()).expect("selection splice length fits u32"),
            &objects,
        );
        run_start = run_end;
    }
}

pub(super) fn review_ready_count(session: &DoctorReviewSession) -> usize {
    session
        .rows()
        .iter()
        .filter(|row| row.state == DoctorReviewRowState::Ready)
        .count()
}

fn debug_assert_store_layout(store: &gio::ListStore, row_count: usize) {
    let row_count = u32::try_from(row_count).expect("review row count fits u32");
    debug_assert!(matches!(
        store.n_items().checked_sub(row_count),
        Some(0 | 1)
    ));
    debug_assert!((0..row_count).all(|position| {
        store
            .item(position)
            .is_some_and(|item| item.is::<glib::BoxedAnyObject>())
    }));
    debug_assert!(
        store.n_items() == row_count
            || store
                .item(row_count)
                .is_some_and(|item| item.is::<gtk4::Widget>())
    );
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
