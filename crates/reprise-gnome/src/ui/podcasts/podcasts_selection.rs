//! Persistent selection state for the rebuilt grouped podcast surface.

use std::collections::BTreeSet;

/// What a click means for the selection. It crosses the action boundary as a
/// `u8`, which keeps one action where three near-identical ones would
/// otherwise be needed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SelectMode {
    Only,
    Toggle,
    Range,
}

impl SelectMode {
    pub(super) const fn as_u8(self) -> u8 {
        match self {
            SelectMode::Only => 0,
            SelectMode::Toggle => 1,
            SelectMode::Range => 2,
        }
    }

    pub(super) const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(SelectMode::Only),
            1 => Some(SelectMode::Toggle),
            2 => Some(SelectMode::Range),
            _ => None,
        }
    }
}

/// `SRC-14`: the selection mechanics both episode surfaces share.
///
/// `order` is the episode ids as they are rendered right now, top to bottom
/// and across group boundaries. A range is defined only over that order:
/// episodes inside a collapsed group, behind a "Show all N" window, or hidden
/// by the active filter are not rendered, so a Shift-click never sweeps them
/// up. The grouped view and the channel detail view own their state
/// differently — one flat set, one set per channel — which is why this takes
/// the pieces rather than a receiver.
pub(super) fn apply_select(
    selected: &mut BTreeSet<i64>,
    anchor: &mut Option<i64>,
    order: &[i64],
    episode_id: i64,
    mode: SelectMode,
) {
    match mode {
        SelectMode::Only => {
            selected.clear();
            selected.insert(episode_id);
            *anchor = Some(episode_id);
        }
        SelectMode::Toggle => {
            if !selected.remove(&episode_id) {
                selected.insert(episode_id);
            }
            *anchor = Some(episode_id);
        }
        SelectMode::Range => {
            let span = anchor
                .and_then(|anchor| position(order, anchor))
                .zip(position(order, episode_id));
            let Some((from, to)) = span else {
                // No anchor, or an anchor that is no longer on screen: the
                // honest fallback is the row the user actually clicked.
                apply_select(selected, anchor, order, episode_id, SelectMode::Only);
                return;
            };
            selected.clear();
            selected.extend(order[from.min(to)..=from.max(to)].iter().copied());
        }
    }
}

fn position(order: &[i64], episode_id: i64) -> Option<usize> {
    order.iter().position(|candidate| *candidate == episode_id)
}

/// `SRC-12b`: every rendered episode of the focused row's source, or the
/// complete rendered order when no row has focus.
pub(super) fn select_all_in_source(order: &[(i64, i64)], focused: Option<i64>) -> Vec<i64> {
    let source = focused.and_then(|focused| {
        order
            .iter()
            .find(|(_, episode_id)| *episode_id == focused)
            .map(|(subscription_id, _)| *subscription_id)
    });
    order
        .iter()
        .filter(|(subscription_id, _)| source.is_none_or(|source| *subscription_id == source))
        .map(|(_, episode_id)| *episode_id)
        .collect()
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct PodcastSelection {
    selected: BTreeSet<i64>,
    anchor: Option<i64>,
}

impl PodcastSelection {
    pub(super) fn apply(&mut self, order: &[i64], episode_id: i64, mode: SelectMode) {
        apply_select(
            &mut self.selected,
            &mut self.anchor,
            order,
            episode_id,
            mode,
        );
    }

    pub(super) fn set_selected(&mut self, episode_id: i64, selected: bool) {
        if selected {
            self.selected.insert(episode_id);
        } else {
            self.selected.remove(&episode_id);
        }
    }

    pub(super) fn replace_with(&mut self, episode_ids: impl IntoIterator<Item = i64>) {
        let episode_ids = episode_ids.into_iter().collect::<Vec<_>>();
        self.anchor = episode_ids.first().copied();
        self.selected = episode_ids.into_iter().collect();
    }

    pub(super) fn selected_ids(&self) -> Vec<i64> {
        self.selected.iter().copied().collect()
    }

    /// Drops every selected episode. Returns whether anything was selected —
    /// the caller uses this to decide whether Escape was consumed.
    pub(super) fn clear(&mut self) -> bool {
        !std::mem::take(&mut self.selected).is_empty()
    }

    pub(super) fn contains(&self, episode_id: i64) -> bool {
        self.selected.contains(&episode_id)
    }

    /// Applies SRC-14's context-menu take-over rule and reports whether the
    /// caller must publish the changed selection to the visible surface.
    pub(super) fn take_over_for_context_menu(&mut self, episode_id: i64) -> bool {
        if self.contains(episode_id) {
            return false;
        }
        self.apply(&[], episode_id, SelectMode::Only);
        true
    }

    pub(super) fn remove_all(&mut self, episode_ids: &[i64]) {
        for episode_id in episode_ids {
            self.selected.remove(episode_id);
        }
    }

    pub(super) fn retain_available(&mut self, available_ids: impl IntoIterator<Item = i64>) {
        let available = available_ids.into_iter().collect::<BTreeSet<_>>();
        self.selected
            .retain(|episode_id| available.contains(episode_id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(selected: &BTreeSet<i64>) -> Vec<i64> {
        selected.iter().copied().collect()
    }

    /// `SRC-12b`: Ctrl+A means the focused source, not every expanded group.
    #[test]
    fn src_12b_ctrl_a_selects_the_focused_sources_rendered_episodes() {
        let order = [(1, 10), (1, 11), (2, 20), (2, 21)];
        assert_eq!(select_all_in_source(&order, Some(11)), vec![10, 11]);
        assert_eq!(select_all_in_source(&order, Some(20)), vec![20, 21]);
    }

    #[test]
    fn src_12b_ctrl_a_without_a_focused_row_takes_the_rendered_list() {
        let order = [(1, 10), (2, 20)];
        assert_eq!(select_all_in_source(&order, None), vec![10, 20]);
    }

    #[test]
    fn src_12b_ctrl_a_cannot_reach_unrendered_episodes() {
        let order = [(1, 10)];
        assert_eq!(select_all_in_source(&order, Some(10)), vec![10]);
    }

    #[test]
    fn src_14_only_replaces_the_selection_and_moves_the_anchor() {
        let mut selected = BTreeSet::from([7, 8]);
        let mut anchor = Some(7);

        apply_select(&mut selected, &mut anchor, &[7, 8, 9], 9, SelectMode::Only);

        assert_eq!(ids(&selected), vec![9]);
        assert_eq!(anchor, Some(9));
    }

    #[test]
    fn src_14_toggle_adds_then_removes_and_moves_the_anchor() {
        let mut selected = BTreeSet::new();
        let mut anchor = None;

        apply_select(
            &mut selected,
            &mut anchor,
            &[7, 8, 9],
            8,
            SelectMode::Toggle,
        );
        assert_eq!(ids(&selected), vec![8]);
        assert_eq!(anchor, Some(8));

        apply_select(
            &mut selected,
            &mut anchor,
            &[7, 8, 9],
            8,
            SelectMode::Toggle,
        );
        assert!(selected.is_empty());
        assert_eq!(anchor, Some(8));
    }

    #[test]
    fn src_14_range_spans_the_rendered_order_in_both_directions() {
        let order = [1, 2, 3, 4, 5];
        let mut selected = BTreeSet::new();
        let mut anchor = None;

        apply_select(&mut selected, &mut anchor, &order, 4, SelectMode::Only);
        apply_select(&mut selected, &mut anchor, &order, 2, SelectMode::Range);
        assert_eq!(
            ids(&selected),
            vec![2, 3, 4],
            "a backwards range still spans"
        );
        assert_eq!(anchor, Some(4), "a range never moves the anchor");

        apply_select(&mut selected, &mut anchor, &order, 5, SelectMode::Range);
        assert_eq!(
            ids(&selected),
            vec![4, 5],
            "the range is re-taken from the anchor, not added to"
        );
    }

    #[test]
    fn src_14_range_without_a_usable_anchor_selects_only_the_clicked_row() {
        let mut selected = BTreeSet::from([1]);
        let mut anchor = None;

        apply_select(&mut selected, &mut anchor, &[1, 2, 3], 3, SelectMode::Range);

        assert_eq!(ids(&selected), vec![3]);
        assert_eq!(anchor, Some(3));

        // An anchor that is no longer rendered — its group was collapsed — is
        // not a usable anchor either.
        let mut selected = BTreeSet::from([9]);
        let mut anchor = Some(9);

        apply_select(&mut selected, &mut anchor, &[1, 2, 3], 2, SelectMode::Range);

        assert_eq!(ids(&selected), vec![2]);
        assert_eq!(anchor, Some(2));
    }

    #[test]
    fn src_14_a_row_outside_the_rendered_order_is_still_selectable() {
        let mut selected = BTreeSet::new();
        let mut anchor = None;

        apply_select(&mut selected, &mut anchor, &[1, 2], 99, SelectMode::Only);

        assert_eq!(ids(&selected), vec![99]);
    }

    #[test]
    fn src_14_select_modes_survive_the_action_round_trip() {
        for mode in [SelectMode::Only, SelectMode::Toggle, SelectMode::Range] {
            assert_eq!(SelectMode::from_u8(mode.as_u8()), Some(mode));
        }
        assert_eq!(SelectMode::from_u8(3), None);
    }

    #[test]
    fn src_14_the_selection_applies_the_shared_mechanics() {
        let mut selection = PodcastSelection::default();

        selection.apply(&[1, 2, 3], 1, SelectMode::Only);
        selection.apply(&[1, 2, 3], 3, SelectMode::Range);

        assert_eq!(selection.selected_ids(), vec![1, 2, 3]);
    }

    #[test]
    fn src_14_context_menu_preserves_a_selection_that_contains_the_clicked_row() {
        let mut selection = PodcastSelection::default();
        selection.set_selected(1, true);
        selection.set_selected(2, true);
        selection.set_selected(3, true);

        assert!(!selection.take_over_for_context_menu(2));
        assert_eq!(selection.selected_ids(), vec![1, 2, 3]);
    }

    #[test]
    fn src_14_context_menu_replaces_a_selection_outside_the_clicked_row() {
        let mut selection = PodcastSelection::default();
        selection.set_selected(1, true);
        selection.set_selected(2, true);

        assert!(selection.take_over_for_context_menu(3));
        assert_eq!(selection.selected_ids(), vec![3]);
    }

    #[test]
    fn src_12b_selection_survives_a_widget_rebuild_and_spans_shows() {
        let mut selection = PodcastSelection::default();
        selection.set_selected(11, true);
        selection.set_selected(21, true);

        selection.retain_available([11, 12, 21, 22]);

        assert_eq!(selection.selected_ids(), [11, 21]);
    }

    #[test]
    fn src_12b_clear_reports_and_drops_a_non_empty_selection() {
        let mut selection = PodcastSelection::default();
        selection.set_selected(11, true);
        selection.set_selected(21, true);

        assert!(selection.clear());
        assert!(selection.selected_ids().is_empty());
    }

    #[test]
    fn src_12b_clear_reports_an_already_empty_selection() {
        let mut selection = PodcastSelection::default();

        assert!(!selection.clear());
        assert!(selection.selected_ids().is_empty());
    }

    #[test]
    fn completing_one_surface_batch_preserves_unrelated_selection() {
        let mut selection = PodcastSelection::default();
        for episode_id in [11, 12, 21] {
            selection.set_selected(episode_id, true);
        }

        selection.remove_all(&[11, 12]);

        assert_eq!(selection.selected_ids(), [21]);
    }
}
