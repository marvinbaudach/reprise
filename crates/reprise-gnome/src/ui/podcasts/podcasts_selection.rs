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

pub(super) struct SelectionControls {
    root: gtk4::Box,
    selected: gtk4::Label,
    download: gtk4::Button,
    remove: gtk4::Button,
}

impl SelectionControls {
    pub(super) fn new() -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        root.add_css_class("toolbar");
        root.set_margin_start(12);
        root.set_margin_end(12);
        let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        root.append(&spacer);
        let selected = gtk4::Label::new(None);
        root.append(&selected);
        let download = gtk4::Button::with_label(&strings::text(strings::YOUTUBE_DOWNLOAD_SELECTED));
        root.append(&download);
        let remove = gtk4::Button::with_label(&strings::text(strings::YOUTUBE_REMOVE_SELECTED));
        remove.add_css_class("destructive-action");
        root.append(&remove);
        let controls = Self {
            root,
            selected,
            download,
            remove,
        };
        controls.update(&[]);
        controls
    }

    pub(super) fn widget(&self) -> &gtk4::Widget {
        self.root.upcast_ref()
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
}
