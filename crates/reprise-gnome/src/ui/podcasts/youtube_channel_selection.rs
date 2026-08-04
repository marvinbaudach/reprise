//! Selection projection for the active YouTube channel page.

use super::*;

impl YoutubeChannelDetail {
    /// `SRC-14`: the range order is the active channel's rendered window.
    pub(super) fn rendered_order(&self) -> Vec<i64> {
        let Some(subscription_id) = self.state.borrow().active_channel() else {
            return Vec::new();
        };
        let Some(rendered) = self
            .groups
            .borrow()
            .iter()
            .find(|group| group.group.subscription_id == subscription_id)
            .cloned()
        else {
            return Vec::new();
        };
        let state = self.state.borrow().clone();
        project_channel(&rendered, &state)
            .group
            .episodes
            .iter()
            .map(|episode| episode.id)
            .collect()
    }

    /// Pushes selection onto retained rows so keyboard focus survives.
    pub(super) fn apply_selection(self: &Rc<Self>) {
        let Some(subscription_id) = self.state.borrow().active_channel() else {
            return;
        };
        let selected = self.state.borrow().selected_ids(subscription_id);
        let widgets = self
            .selection_widgets
            .borrow()
            .iter()
            .map(|(episode_id, widgets)| (*episode_id, widgets.row.clone(), widgets.reveal.clone()))
            .collect::<Vec<_>>();
        for (episode_id, row, reveal) in widgets {
            let is_selected = selected.contains(&episode_id);
            if is_selected {
                row.add_css_class(SELECTED_ROW_CLASS);
            } else {
                row.remove_css_class(SELECTED_ROW_CLASS);
            }
            row.update_state(&[gtk4::accessible::State::Selected(Some(is_selected))]);
            if let Some(reveal) = reveal {
                reveal.set_selected(is_selected);
            }
        }
        let summary = self
            .selection_summary
            .borrow()
            .as_ref()
            .map(|summary| (summary.label.clone(), summary.base.clone()));
        if let Some((label, base)) = summary {
            label.set_text(&strings::podcast_summary_with_selection(
                &base,
                selected.len(),
            ));
        }
    }

    pub(super) fn select_row(self: &Rc<Self>, episode_id: i64, mode: SelectMode) {
        let Some(subscription_id) = self.state.borrow().active_channel() else {
            return;
        };
        let order = self.rendered_order();
        self.state
            .borrow_mut()
            .apply_select(subscription_id, &order, episode_id, mode);
        self.apply_selection();
    }

    /// `SRC-12a`: this page has one unambiguous source, so Ctrl+A takes its
    /// rendered window and cannot reach filtered or not-yet-loaded episodes.
    pub(in crate::ui::podcasts) fn select_all_visible(self: &Rc<Self>) -> bool {
        let Some(subscription_id) = self.state.borrow().active_channel() else {
            return false;
        };
        let order = self.rendered_order();
        let selection = self.state.borrow_mut().selection(subscription_id);
        selection.borrow_mut().replace_with(order);
        self.apply_selection();
        true
    }
}
