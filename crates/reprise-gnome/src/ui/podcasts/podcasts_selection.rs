//! Persistent selection state for the rebuilt grouped podcast surface.

use std::collections::BTreeSet;

use gtk4::glib::variant::ToVariant;
use gtk4::prelude::*;

use crate::ui::strings;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct PodcastSelection {
    selected: BTreeSet<i64>,
}

impl PodcastSelection {
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
