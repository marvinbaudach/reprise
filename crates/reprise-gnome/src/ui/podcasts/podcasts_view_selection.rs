//! `SRC-14`: how a selection is made and shown in the grouped library view.
//! Split out of `podcasts_view.rs` to keep it under the file-size gate.

use super::*;

impl PodcastsView {
    /// `SRC-14`: the episode order a range selection ranges over, read fresh
    /// on every use. A group's expander writes `expanded_sources` straight
    /// from its `notify` handler without a re-render, so a cached order would
    /// be stale the moment a user opened or closed a group.
    pub(super) fn rendered_order(&self) -> Vec<i64> {
        podcasts_rendered_order::rendered_episode_ids(
            &self.groups.borrow(),
            &self.expanded_sources.borrow(),
            &self.expanded_episode_sources.borrow(),
        )
    }

    /// `SRC-14`: push the current selection onto the rows already on screen.
    ///
    /// Deliberately not a `render()`: rebuilding every row drops keyboard
    /// focus, which would make selecting a second row with the keyboard
    /// impossible — the focused row would no longer exist.
    pub(super) fn apply_selection(&self) {
        let selection = self.selection.borrow();
        for (episode_id, widgets) in self.selection_widgets.borrow().iter() {
            let selected = selection.contains(*episode_id);
            if selected {
                widgets
                    .row
                    .add_css_class(podcasts_groups::SELECTED_ROW_CLASS);
            } else {
                widgets
                    .row
                    .remove_css_class(podcasts_groups::SELECTED_ROW_CLASS);
            }
            widgets
                .row
                .update_state(&[gtk4::accessible::State::Selected(Some(selected))]);
        }
        self.filter_bar
            .set_selection_count(selection.selected_ids().len());
    }

    pub(super) fn select_row(&self, episode_id: i64, mode: SelectMode) {
        let order = self.rendered_order();
        self.selection.borrow_mut().apply(&order, episode_id, mode);
        self.apply_selection();
    }
}
