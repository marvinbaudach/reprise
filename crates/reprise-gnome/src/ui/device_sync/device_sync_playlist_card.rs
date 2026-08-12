//! The playlist-selection card extracted for the playlist-card rebuild.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;

use super::device_sync_page_copy::{counted, playlist_subtitle};
use super::device_sync_runtime::DeviceView;
use super::device_sync_strings;

#[derive(Clone)]
pub(super) struct PlaylistCardActions {
    pub(super) set_playlist: Rc<dyn Fn(reprise_core::device_sync::SelectionSource, bool)>,
    pub(super) open_picker: Rc<dyn Fn(gtk4::Widget)>,
}

#[derive(Clone)]
pub(super) struct PlaylistRowWidgets {
    source: reprise_core::device_sync::SelectionSource,
    pub(super) button: gtk4::ToggleButton,
    pub(super) title: gtk4::Label,
    subtitle: gtk4::Label,
    indicator: gtk4::Label,
}

pub(super) struct PlaylistCard {
    root: libadwaita::Bin,
    pub(super) list: gtk4::ListBox,
    summary: gtk4::Label,
    pub(super) rows: RefCell<Vec<PlaylistRowWidgets>>,
    updating: Rc<Cell<bool>>,
    actions: PlaylistCardActions,
}

impl PlaylistCard {
    pub(super) fn new(actions: PlaylistCardActions) -> Self {
        let title = label(
            &device_sync_strings::text(device_sync_strings::PLAYLISTS),
            "title-2",
        );
        let summary = label("", "dim-label");
        summary.set_halign(gtk4::Align::End);
        summary.set_hexpand(true);
        let choose = gtk4::Button::with_label(&device_sync_strings::text(
            device_sync_strings::CHOOSE_PLAYLISTS,
        ));
        {
            let open_picker = actions.open_picker.clone();
            choose.connect_clicked(move |button| open_picker(button.clone().upcast()));
        }
        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        header.append(&title);
        header.append(&summary);
        header.append(&choose);

        let list = gtk4::ListBox::new();
        list.set_show_separators(true);
        list.set_selection_mode(gtk4::SelectionMode::None);
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        content.set_margin_top(16);
        content.set_margin_bottom(16);
        content.set_margin_start(18);
        content.set_margin_end(18);
        content.append(&header);
        content.append(&list);
        let root = libadwaita::Bin::builder().child(&content).build();
        root.add_css_class("card");
        root.set_hexpand(true);

        Self {
            root,
            list,
            summary,
            rows: RefCell::new(Vec::new()),
            updating: Rc::new(Cell::new(false)),
            actions,
        }
    }

    pub(super) fn root(&self) -> &gtk4::Widget {
        self.root.upcast_ref()
    }

    pub(super) fn update(&self, device: &DeviceView) {
        self.updating.set(true);
        let sources = device
            .page
            .playlists
            .iter()
            .map(|playlist| playlist.source.clone())
            .collect::<Vec<_>>();
        let existing_rows = self.rows.borrow().iter().cloned().collect::<Vec<_>>();
        let current = existing_rows
            .iter()
            .map(|row| row.source.clone())
            .collect::<Vec<_>>();
        if sources != current {
            let focused = existing_rows
                .iter()
                .enumerate()
                .find(|(_, row)| row.button.is_focus() || row.button.has_focus())
                .map(|(index, row)| (index, row.source.clone()));
            let old_rows = self
                .rows
                .borrow_mut()
                .drain(..)
                .map(|row| row.button)
                .collect::<Vec<_>>();
            for row in old_rows {
                self.list.remove(&row);
            }
            for playlist in &device.page.playlists {
                let button = gtk4::ToggleButton::new();
                button.add_css_class("flat");
                button.set_hexpand(true);
                let indicator = gtk4::Label::new(Some("☐"));
                indicator.add_css_class("title-3");
                let title = gtk4::Label::new(None);
                title.set_xalign(0.0);
                let subtitle = gtk4::Label::new(None);
                subtitle.add_css_class("dim-label");
                subtitle.set_xalign(0.0);
                subtitle.set_wrap(true);
                let labels = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
                labels.set_hexpand(true);
                labels.append(&title);
                labels.append(&subtitle);
                let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
                content.set_margin_top(10);
                content.set_margin_bottom(10);
                content.set_margin_start(12);
                content.set_margin_end(12);
                content.append(&indicator);
                content.append(&labels);
                button.set_child(Some(&content));
                let source = playlist.source.clone();
                let updating = self.updating.clone();
                let set_playlist = self.actions.set_playlist.clone();
                button.connect_toggled(move |button| {
                    if !updating.get() {
                        set_playlist(source.clone(), button.is_active());
                    }
                });
                self.list.append(&button);
                self.rows.borrow_mut().push(PlaylistRowWidgets {
                    source: playlist.source.clone(),
                    button,
                    title,
                    subtitle,
                    indicator,
                });
            }
            if let Some((old_index, focused_source)) = focused {
                let rebuilt_rows = self.rows.borrow().iter().cloned().collect::<Vec<_>>();
                let target = rebuilt_rows
                    .iter()
                    .find(|row| row.source == focused_source)
                    .or_else(|| {
                        rebuilt_rows.get(old_index.min(rebuilt_rows.len().saturating_sub(1)))
                    });
                if let Some(row) = target {
                    row.button.grab_focus();
                }
            }
        }
        let rows = self.rows.borrow().iter().cloned().collect::<Vec<_>>();
        for (row, playlist) in rows.iter().zip(&device.page.playlists) {
            let name = playlist.name.as_deref().unwrap_or("Unavailable playlist");
            row.title.set_label(name);
            row.subtitle.set_label(&playlist_subtitle(playlist));
            row.button.set_active(playlist.selected);
            row.indicator
                .set_label(if playlist.selected { "☑" } else { "☐" });
            row.button
                .update_property(&[gtk4::accessible::Property::Label(name)]);
            row.button.set_sensitive(device.page.controls.editable);
        }
        self.summary.set_label(&format!(
            "{} · {} on device",
            counted(
                device.page.unique_track_count,
                "unique track",
                "unique tracks"
            ),
            device_sync_strings::file_size(device.page.target_bytes)
        ));
        self.updating.set(false);
    }

    pub(super) fn focus_first(&self) {
        if let Some(row) = self.rows.borrow().first() {
            row.button.grab_focus();
        }
    }

    pub(super) fn focus_widget(&self) -> Option<gtk4::Widget> {
        self.rows
            .borrow()
            .iter()
            .find(|row| row.button.is_sensitive())
            .map(|row| row.button.clone().upcast())
    }
}

fn label(text: &str, class: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.set_xalign(0.0);
    label.add_css_class(class);
    label
}
