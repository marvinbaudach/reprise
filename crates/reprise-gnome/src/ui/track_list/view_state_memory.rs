//! BROWSE-2 projection between the live TrackList and a history-owned
//! `TrackViewState`. Capture and restore include local search, browse facets,
//! sort, stable ID-plus-offset anchor, selection, and content focus. No widget
//! state map lives beside the core router: Back/Forward carries the complete
//! place. Scroll is never stored as an absolute pixel value.

use gtk4::prelude::*;
use reprise_core::browser::TrackFocus;
use reprise_core::browser::{SortDirection, TrackAnchor, TrackSort, TrackViewState};
use reprise_core::queries::BrowseFilter;

#[cfg(test)]
use crate::ui::list_geometry_layout::ListLayout;
use crate::ui::track_list::Shared;
use crate::ui::track_list_sort::SortState;

/// Upper bound on remembered selected ids per source — a guard against a
/// pathological select-all on a 10k-track view being cloned around on every
/// source switch. Restoring the first 512 of such a selection is fine; the
/// point of BROWSE-2 is orientation, not perfect multi-selection fidelity.
const MAX_REMEMBERED_SELECTED_IDS: usize = 512;

/// GTK-side representation of one complete track place.
#[derive(Clone, Debug, Default, PartialEq)]
pub(in crate::ui) struct SavedViewState {
    pub search: String,
    pub browse: BrowseFilter,
    pub sort: SortState,
    pub anchor: Option<(i64, f64)>,
    pub selected_ids: Vec<i64>,
    pub focus: TrackFocus,
}

impl SavedViewState {
    pub(in crate::ui) fn to_core(&self) -> TrackViewState {
        TrackViewState {
            search: self.search.clone(),
            browse: self.browse.clone(),
            sort: TrackSort::new(
                &self.sort.field,
                if self.sort.dir == "desc" {
                    SortDirection::Descending
                } else {
                    SortDirection::Ascending
                },
            ),
            anchor: self
                .anchor
                .map(|(track_id, row_offset)| TrackAnchor::new(track_id, row_offset)),
            selected_ids: self.selected_ids.clone(),
            focus: self.focus,
        }
    }

    pub(in crate::ui) fn from_core(state: &TrackViewState) -> Self {
        Self {
            search: state.search.clone(),
            browse: state.browse.clone(),
            sort: SortState {
                field: state.sort.field.clone(),
                dir: match state.sort.direction {
                    SortDirection::Ascending => "asc",
                    SortDirection::Descending => "desc",
                }
                .into(),
            },
            anchor: state
                .anchor
                .map(|anchor| (anchor.track_id, anchor.row_offset)),
            selected_ids: state.selected_ids.clone(),
            focus: state.focus,
        }
    }
}

/// Maps remembered selected ids onto positions in the view's *current* id
/// list. Ids that no longer exist in the view (deleted, filtered away by a
/// changed smart playlist, …) are silently dropped; surviving ids map to
/// their new positions even when sorting moved them.
pub(in crate::ui) fn positions_to_select(saved: &SavedViewState, current_ids: &[i64]) -> Vec<u32> {
    let remembered: std::collections::HashSet<i64> = saved.selected_ids.iter().copied().collect();
    current_ids
        .iter()
        .enumerate()
        .filter(|(_, id)| remembered.contains(id))
        .filter_map(|(position, _)| u32::try_from(position).ok())
        .collect()
}

#[cfg(test)]
fn scroll_target(
    saved: &SavedViewState,
    current_ids: &[i64],
    layout: &ListLayout,
    viewport_height: f64,
) -> Option<f64> {
    super::reload_restore::scroll_target(saved.anchor, current_ids, layout, viewport_height)
}

/// Captures the current scroll offset + selection of the track table.
pub(in crate::ui) fn capture(shared: &Shared) -> SavedViewState {
    let scroll_value = gtk4::prelude::ScrollableExt::vadjustment(&shared.column_view)
        .map_or(0.0, |adjustment| adjustment.value());
    let total = shared.model.n_items();
    let anchor =
        super::track_list_geometry::layout(shared, None, total as usize).and_then(|layout| {
            let (position, offset) = layout.row_at(scroll_value.max(0.0));
            shared
                .model
                .track_at(position)
                .map(|track| (track.id, offset))
        });
    let selection = shared.selection.selection();
    let mut selected_ids = Vec::new();
    for index in 0..selection.size() {
        if selected_ids.len() >= MAX_REMEMBERED_SELECTED_IDS {
            break;
        }
        let position = selection.nth(u32::try_from(index).unwrap_or(u32::MAX));
        if let Some(track) = shared.model.track_at(position) {
            selected_ids.push(track.id);
        }
    }
    let focus = selected_ids
        .first()
        .copied()
        .filter(|_| shared.column_view.has_focus())
        .map_or(TrackFocus::Content, TrackFocus::Track);
    SavedViewState {
        search: shared.filter.borrow().clone(),
        browse: shared.browse_filter.borrow().clone(),
        sort: shared.sort.borrow().clone(),
        anchor,
        selected_ids,
        focus,
    }
}

pub(in crate::ui) fn capture_place(shared: &Shared) -> reprise_core::browser::BrowserPlace {
    let source = shared.source.borrow().clone();
    let state = capture(shared).to_core();
    let collection = reprise_core::browser::BrowserPlace::from(source)
        .collection()
        .cloned()
        .unwrap_or(reprise_core::browser::TrackCollection::Library(
            reprise_core::browser::LibraryScope::All,
        ));
    reprise_core::browser::BrowserPlace::tracks(collection, state)
}

/// Restores a captured view state after `reload` rebuilt the model for the
/// re-attached source: selection synchronously (the model rows exist), the
/// scroll offset once the rebuilt list reports changed geometry.
pub(in crate::ui) fn restore(
    shared: &std::rc::Rc<Shared>,
    saved: &SavedViewState,
    current_ids: &[i64],
) {
    let positions = positions_to_select(saved, current_ids);
    shared.selection.unselect_all();
    for position in positions {
        shared.selection.select_item(position, false);
    }
    restore_scroll_when_ready(shared, saved.anchor, current_ids.to_vec());
    if matches!(saved.focus, TrackFocus::Track(_)) {
        let _ = shared.column_view.grab_focus();
    }
}

/// Applies the remembered value immediately when geometry is already usable,
/// otherwise on the adjustment's next geometry change. A list that fits its
/// viewport entirely needs no scroll at all.
fn restore_scroll_when_ready(
    shared: &std::rc::Rc<Shared>,
    anchor: Option<(i64, f64)>,
    current_ids: Vec<i64>,
) {
    if anchor.is_none() || current_ids.is_empty() {
        return;
    }
    if apply_restored_scroll(shared, anchor, &current_ids) {
        return;
    }
    let Some(adjustment) = gtk4::prelude::ScrollableExt::vadjustment(&shared.column_view) else {
        return;
    };
    let weak_shared = std::rc::Rc::downgrade(shared);
    let generation = shared.model.generation();
    crate::ui::list_geometry_changed::after_changed_once(&adjustment, move || {
        let Some(shared) = weak_shared.upgrade() else {
            return;
        };
        if shared.model.generation() != generation {
            return;
        }
        apply_restored_scroll(&shared, anchor, &current_ids);
    });
}

fn apply_restored_scroll(shared: &Shared, anchor: Option<(i64, f64)>, current_ids: &[i64]) -> bool {
    let Some(adjustment) = gtk4::prelude::ScrollableExt::vadjustment(&shared.column_view) else {
        return false;
    };
    let (upper, page) = (adjustment.upper(), adjustment.page_size());
    if upper <= page {
        return true;
    }
    let Some(layout) = super::track_list_geometry::layout(shared, None, current_ids.len()) else {
        return false;
    };
    let Some(target) = super::reload_restore::scroll_target(anchor, current_ids, &layout, page)
    else {
        return false;
    };
    crate::ui::scroll_probe::probe("view_state_restore", &adjustment, target);
    debug_assert!(
        !crate::ui::list_geometry_changed::in_changed_emission(),
        "view-state scroll written from inside a changed emission"
    );
    adjustment.set_value(target);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved(ids: &[i64]) -> SavedViewState {
        SavedViewState {
            search: String::new(),
            browse: BrowseFilter::default(),
            sort: SortState::default(),
            anchor: None,
            selected_ids: ids.to_vec(),
            focus: TrackFocus::Content,
        }
    }

    #[test]
    fn browse_2_remembers_scroll_and_selection_per_place() {
        let library = SavedViewState {
            search: "shore".into(),
            browse: reprise_core::queries::BrowseFilter {
                genre: Some("Metal".into()),
                ..reprise_core::queries::BrowseFilter::default()
            },
            sort: crate::ui::track_list_sort::SortState {
                field: "album".into(),
                dir: "desc".into(),
            },
            anchor: Some((42, 7.5)),
            selected_ids: vec![42, 99],
            focus: reprise_core::browser::TrackFocus::Track(42),
        };
        let playlist = SavedViewState {
            search: String::new(),
            browse: reprise_core::queries::BrowseFilter::default(),
            sort: crate::ui::track_list_sort::SortState::default(),
            anchor: Some((7, 2.0)),
            selected_ids: vec![7],
            focus: reprise_core::browser::TrackFocus::Content,
        };
        let mut memory = std::collections::HashMap::new();
        memory.insert("tracks", library.clone());
        memory.insert("playlist", playlist.clone());

        assert_eq!(memory.get("tracks"), Some(&library));
        assert_eq!(memory.get("playlist"), Some(&playlist));
        assert_eq!(library.search, "shore");
        assert_eq!(library.browse.genre.as_deref(), Some("Metal"));
        assert_eq!(library.sort.field, "album");
        assert_eq!(library.focus, reprise_core::browser::TrackFocus::Track(42));
    }

    #[test]
    fn browse_2_anchor_survives_resort() {
        let state = SavedViewState {
            search: String::new(),
            browse: reprise_core::queries::BrowseFilter::default(),
            sort: crate::ui::track_list_sort::SortState::default(),
            anchor: Some((42, 6.0)),
            selected_ids: vec![42],
            focus: reprise_core::browser::TrackFocus::Content,
        };
        let resorted = [5, 9, 11, 42, 77, 88];

        let layout = ListLayout::rows_only(crate::ui::list_geometry::RowHeight::new(20.0).unwrap());
        assert_eq!(scroll_target(&state, &resorted, &layout, 40.0), Some(66.0));
    }

    #[test]
    fn positions_to_select_maps_surviving_ids_only() {
        // 11 vanished; 9 and 7 moved — restore selects their new positions.
        assert_eq!(
            positions_to_select(&saved(&[7, 9, 11]), &[9, 42, 7]),
            vec![0, 2]
        );
    }

    #[test]
    fn positions_to_select_handles_empty_saved_and_empty_view() {
        assert_eq!(positions_to_select(&saved(&[]), &[1, 2]), Vec::<u32>::new());
        assert_eq!(positions_to_select(&saved(&[1]), &[]), Vec::<u32>::new());
    }

    #[test]
    fn browse_2_ui_state_round_trips_through_the_core_browser_place() {
        let saved = SavedViewState {
            search: "shore".into(),
            browse: BrowseFilter {
                album: Some("Pain Remains".into()),
                ..BrowseFilter::default()
            },
            sort: SortState {
                field: "year".into(),
                dir: "desc".into(),
            },
            anchor: Some((42, 3.5)),
            selected_ids: vec![42, 44],
            focus: TrackFocus::Track(42),
        };

        assert_eq!(SavedViewState::from_core(&saved.to_core()), saved);
    }
}
