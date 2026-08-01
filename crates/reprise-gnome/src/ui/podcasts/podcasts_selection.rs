//! Persistent selection state for the rebuilt grouped podcast surface.

use std::collections::BTreeSet;

use gtk4::glib::variant::ToVariant;
use gtk4::prelude::*;

use crate::ui::strings;

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

    pub(super) fn selected_ids(&self) -> Vec<i64> {
        self.selected.iter().copied().collect()
    }

    pub(super) fn contains(&self, episode_id: i64) -> bool {
        self.selected.contains(&episode_id)
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

pub(super) fn episode_checkbox(episode_id: i64, title: &str, active: bool) -> gtk4::CheckButton {
    let checkbox = gtk4::CheckButton::new();
    checkbox.set_active(active);
    checkbox.set_tooltip_text(Some(&strings::text(strings::YOUTUBE_SELECT_EPISODES)));
    // The accessible name names the episode; the tooltip stays generic because
    // it is read alongside the row's own visible title anyway.
    checkbox.update_property(&[gtk4::accessible::Property::Label(
        &strings::podcast_select_episode(title),
    )]);
    checkbox.connect_toggled(move |checkbox| {
        let target = (episode_id, checkbox.is_active()).to_variant();
        let _ = checkbox.activate_action("podcasts.set-selected", Some(&target));
    });
    checkbox
}

/// The "N selected / Download selected / Remove selected" trio.
///
/// Both episode surfaces show it, so it is built once here. The grouped
/// library view puts it on a toolbar row of its own; the channel detail view
/// appends it to the end of the toolbar it already has. Only the container
/// differs — the widgets, the sensitivity rule and the action targets do not,
/// which is the whole point of this type existing.
pub(super) struct SelectionControls {
    selected: gtk4::Label,
    download: gtk4::Button,
    remove: gtk4::Button,
}

impl SelectionControls {
    fn build() -> Self {
        let selected = gtk4::Label::new(None);
        let download = gtk4::Button::with_label(&strings::text(strings::YOUTUBE_DOWNLOAD_SELECTED));
        let remove = gtk4::Button::with_label(&strings::text(strings::YOUTUBE_REMOVE_SELECTED));
        remove.add_css_class("destructive-action");
        let controls = Self {
            selected,
            download,
            remove,
        };
        controls.update(&[]);
        controls
    }

    /// The trio on a toolbar row of its own, right-aligned — the grouped
    /// library view, which has no other toolbar to join.
    pub(super) fn standalone() -> (gtk4::Widget, Self) {
        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        root.add_css_class("toolbar");
        root.set_margin_start(12);
        root.set_margin_end(12);
        let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        root.append(&spacer);
        let controls = Self::build();
        controls.append_to(&root);
        (root.upcast(), controls)
    }

    /// The trio appended to an existing toolbar — the channel detail view,
    /// whose bar already carries the window summary, "Load more" and
    /// "Hide Shorts".
    pub(super) fn appended_to(container: &gtk4::Box) -> Self {
        let controls = Self::build();
        controls.append_to(container);
        controls
    }

    fn append_to(&self, container: &gtk4::Box) {
        container.append(&self.selected);
        container.append(&self.download);
        container.append(&self.remove);
    }

    pub(super) fn update(&self, episode_ids: &[i64]) {
        self.selected
            .set_text(&strings::youtube_selected_count(episode_ids.len()));
        let has_selection = !episode_ids.is_empty();
        self.download.set_sensitive(has_selection);
        self.remove.set_sensitive(has_selection);
        self.download
            .set_action_name(Some("podcasts.download-selected"));
        self.download
            .set_action_target_value(Some(&episode_ids.to_variant()));
        self.remove
            .set_action_name(Some("podcasts.remove-selected"));
        self.remove
            .set_action_target_value(Some(&episode_ids.to_variant()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(selected: &BTreeSet<i64>) -> Vec<i64> {
        selected.iter().copied().collect()
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

        apply_select(&mut selected, &mut anchor, &[7, 8, 9], 8, SelectMode::Toggle);
        assert_eq!(ids(&selected), vec![8]);
        assert_eq!(anchor, Some(8));

        apply_select(&mut selected, &mut anchor, &[7, 8, 9], 8, SelectMode::Toggle);
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
        assert_eq!(ids(&selected), vec![2, 3, 4], "a backwards range still spans");
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
    fn src_12_selection_survives_a_widget_rebuild_and_spans_shows() {
        let mut selection = PodcastSelection::default();
        selection.set_selected(11, true);
        selection.set_selected(21, true);

        selection.retain_available([11, 12, 21, 22]);

        assert_eq!(selection.selected_ids(), [11, 21]);
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

    /// SRC-12: the two surfaces must reach the same actions with the same
    /// targets. They do so by construction now — both go through
    /// `SelectionControls` — and this pins that the standalone and appended
    /// constructors really do produce the identical wiring, so the channel
    /// detail cannot quietly drift back to its own copy.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn src_12_both_surfaces_wire_the_same_batch_actions() {
        gtk4::init().unwrap();
        let (_bar, standalone) = SelectionControls::standalone();
        let container = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        let appended = SelectionControls::appended_to(&container);

        for controls in [&standalone, &appended] {
            controls.update(&[4, 8]);
            assert_eq!(
                controls.download.action_name().as_deref(),
                Some("podcasts.download-selected")
            );
            assert_eq!(
                controls.remove.action_name().as_deref(),
                Some("podcasts.remove-selected")
            );
            assert_eq!(
                controls
                    .remove
                    .action_target_value()
                    .and_then(|value| value.get::<Vec<i64>>()),
                Some(vec![4, 8])
            );
            assert!(controls.download.is_sensitive());
        }

        standalone.update(&[]);
        assert!(!standalone.download.is_sensitive());
        assert!(!standalone.remove.is_sensitive());
    }
}
