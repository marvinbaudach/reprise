//! Full-page per-device surface for Android playlist mirroring.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::device_sync::{primary_action, DeviceStorageAccess, TransferProfile};

use super::device_sync_content_panel::{ContentPanel, ContentPanelActions};
use super::device_sync_page_actions::PageActions;
use super::device_sync_page_copy::{
    action_copy, blocker_summary, change_summary, counted, device_last_sync_copy, eject_sensitive,
    playlist_subtitle, profile_label, progress_copy, warning_summary,
};
// Only named directly by this module's own `#[cfg(test)]` child (below) —
// a plain `cargo build` never compiles that module, so these would
// otherwise warn as unused.
#[cfg(test)]
use super::device_sync_page_copy::{transfer_progress_copy, verification_summary, PageActionCopy};
use super::device_sync_page_layout;
use super::device_sync_runtime::{DeviceSyncRuntime, DeviceSyncState, DeviceView};
use super::device_sync_storage_bar::StorageBar;
use super::device_sync_storage_copy::{storage_access_notice, storage_summary};
use super::device_sync_strings;

#[derive(Clone)]
struct PlaylistRowWidgets {
    source: reprise_core::device_sync::SelectionSource,
    button: gtk4::ToggleButton,
    title: gtk4::Label,
    subtitle: gtk4::Label,
    indicator: gtk4::Label,
}

struct DeviceSyncPage {
    root: gtk4::glib::WeakRef<gtk4::Stack>,
    /// Container for the "Recent transfers" card (MTP-20).
    history: gtk4::Box,
    device_name: gtk4::Label,
    connection: gtk4::Label,
    device_last_sync: gtk4::Label,
    profile: gtk4::DropDown,
    playlist_list: gtk4::ListBox,
    playlist_summary: gtk4::Label,
    playlist_rows: RefCell<Vec<PlaylistRowWidgets>>,
    changes: gtk4::Label,
    storage_name: gtk4::Label,
    storage_summary: gtk4::Label,
    storage_bar: StorageBar,
    notice_box: gtk4::Box,
    notice_title: gtk4::Label,
    notice_detail: gtk4::Label,
    preparation_box: gtk4::Box,
    preparation_detail: gtk4::Label,
    progress_box: gtk4::Box,
    progress_title: gtk4::Label,
    progress_detail: gtk4::Label,
    progress_speed: gtk4::Label,
    progress_bar: gtk4::ProgressBar,
    primary: gtk4::Button,
    eject: gtk4::Button,
    content_panel: ContentPanel,
    updating: Rc<Cell<bool>>,
    cancelling: Rc<Cell<bool>>,
    actions: PageActions,
}

impl DeviceSyncPage {
    fn new(
        device: &DeviceView,
        actions: PageActions,
        content_actions: &ContentPanelActions,
    ) -> (Rc<Self>, gtk4::Stack) {
        let labels = device_sync_page_layout::profile_labels(profile_label);
        let dashboard = device_sync_page_layout::build(device, &labels);
        dashboard
            .eject
            .set_tooltip_text(Some(&device_sync_strings::eject_tooltip(false)));
        let content_panel = ContentPanel::new(content_actions);
        dashboard.content.append(content_panel.root());

        let disconnected = adw::StatusPage::builder()
            .icon_name("phone-symbolic")
            .title("Device disconnected")
            .description("Reconnect the device to continue synchronization.")
            .build();
        let root = gtk4::Stack::new();
        root.add_named(&dashboard.root, Some("connected"));
        root.add_named(&disconnected, Some("disconnected"));
        root.set_visible_child_name("connected");
        let root_ref = gtk4::glib::WeakRef::new();
        root_ref.set(Some(&root));

        let updating = Rc::new(Cell::new(false));
        {
            let updating = updating.clone();
            let set_profile = actions.set_profile.clone();
            dashboard.profile.connect_selected_notify(move |row| {
                if updating.get() {
                    return;
                }
                let Some(profile) = TransferProfile::ALL.get(row.selected() as usize).copied()
                else {
                    return;
                };
                set_profile(profile);
            });
        }
        let cancelling = Rc::new(Cell::new(false));
        {
            let cancelling = cancelling.clone();
            let start = actions.start.clone();
            let cancel = actions.cancel.clone();
            dashboard.primary.connect_clicked(move |_| {
                if cancelling.get() {
                    cancel();
                } else {
                    start();
                }
            });
        }
        {
            let eject_action = actions.eject.clone();
            dashboard.eject.connect_clicked(move |_| eject_action());
        }

        let surface = Rc::new(Self {
            root: root_ref,
            history: dashboard.history,
            device_name: dashboard.device_name,
            connection: dashboard.connection,
            device_last_sync: dashboard.device_last_sync,
            profile: dashboard.profile,
            playlist_list: dashboard.playlist_list,
            playlist_summary: dashboard.playlist_summary,
            playlist_rows: RefCell::new(Vec::new()),
            changes: dashboard.changes,
            storage_name: dashboard.storage_name,
            storage_summary: dashboard.storage_summary,
            storage_bar: dashboard.storage_bar,
            notice_box: dashboard.notice_box,
            notice_title: dashboard.notice_title,
            notice_detail: dashboard.notice_detail,
            preparation_box: dashboard.preparation_box,
            preparation_detail: dashboard.preparation_detail,
            progress_box: dashboard.progress_box,
            progress_title: dashboard.progress_title,
            progress_detail: dashboard.progress_detail,
            progress_speed: dashboard.progress_speed,
            progress_bar: dashboard.progress_bar,
            primary: dashboard.primary,
            eject: dashboard.eject,
            content_panel,
            updating,
            cancelling,
            actions,
        });
        surface.update(device);
        // The widget tree owns its controller, while the controller keeps only
        // a weak root reference. Dropping the removed root therefore releases
        // both the controller and its runtime callback without a cycle.
        let keepalive = surface.clone();
        root.connect_unrealize(move |_| {
            let _ = &keepalive;
        });
        (surface, root)
    }

    fn update(&self, device: &DeviceView) {
        super::device_sync_history::fill(&self.history, &device.history);
        self.updating.set(true);
        self.device_name.set_label(&device.name);
        self.connection.remove_css_class("success");
        self.connection.remove_css_class("warning");
        match &device.session_state {
            reprise_core::device_sync::DeviceSessionState::Active => {
                self.connection.set_label("MTP connected");
                self.connection.add_css_class("success");
            }
            reprise_core::device_sync::DeviceSessionState::Inert { active_device_name } => {
                self.connection
                    .set_label(&device_sync_strings::inert_device_status(
                        active_device_name,
                    ));
                self.connection.add_css_class("warning");
            }
        }
        self.device_last_sync
            .set_label(&device_last_sync_copy(device));
        if let Some(root) = self.root.upgrade() {
            root.set_visible_child_name(if device.connected {
                "connected"
            } else {
                "disconnected"
            });
        }
        let selected = TransferProfile::ALL
            .iter()
            .position(|profile| profile == &device.page.profile)
            .unwrap_or(0);
        self.profile.set_selected(selected as u32);
        self.profile.set_sensitive(device.page.controls.editable);
        self.update_playlists(device);
        self.playlist_summary.set_label(&format!(
            "{} · {} on device",
            counted(
                device.page.unique_track_count,
                "unique track",
                "unique tracks"
            ),
            device_sync_strings::file_size(device.page.target_bytes)
        ));
        self.changes
            .set_label(&change_summary(&device.page.changes));
        self.storage_name.set_label(
            device
                .page
                .storage
                .target_name
                .as_deref()
                .unwrap_or("Device storage"),
        );
        self.storage_summary
            .set_label(&storage_summary(&device.page.storage));
        self.storage_bar.update(&device.page.storage);
        self.update_notice(device);
        self.update_preparation(device);
        self.update_progress(device);
        self.update_actions(device);
        self.content_panel.update(device);
        self.updating.set(false);
    }

    fn update_playlists(&self, device: &DeviceView) {
        let sources = device
            .page
            .playlists
            .iter()
            .map(|playlist| playlist.source.clone())
            .collect::<Vec<_>>();
        let existing_rows = self
            .playlist_rows
            .borrow()
            .iter()
            .cloned()
            .collect::<Vec<_>>();
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
                .playlist_rows
                .borrow_mut()
                .drain(..)
                .map(|row| row.button)
                .collect::<Vec<_>>();
            for row in old_rows {
                self.playlist_list.remove(&row);
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
                self.playlist_list.append(&button);
                self.playlist_rows.borrow_mut().push(PlaylistRowWidgets {
                    source: playlist.source.clone(),
                    button,
                    title,
                    subtitle,
                    indicator,
                });
            }
            if let Some((old_index, focused_source)) = focused {
                let rebuilt_rows = self
                    .playlist_rows
                    .borrow()
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>();
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
        let rows = self
            .playlist_rows
            .borrow()
            .iter()
            .cloned()
            .collect::<Vec<_>>();
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
    }

    fn update_notice(&self, device: &DeviceView) {
        let mut notices = Vec::new();
        if let Some(blocker) = blocker_summary(&device.page.blockers) {
            notices.push(blocker);
        }
        if let Some(access_notice) = storage_access_notice(device.page.storage.access) {
            notices.push(access_notice);
        }
        notices.extend(warning_summary(&device.page.warnings));
        if let Some(error) = &device.scan_error {
            notices.push(format!("Could not inspect device storage: {error}"));
        }
        if let Some(error) = &device.sync_error {
            notices.push(error.message.clone());
        }
        self.notice_box.set_visible(!notices.is_empty());
        let storage_blocks = device.page.storage.access == DeviceStorageAccess::ReadOnly;
        self.notice_title
            .set_label(if !device.page.blockers.is_empty() || storage_blocks {
                "Synchronization blocked"
            } else {
                "Attention"
            });
        self.notice_detail.set_label(&notices.join("\n"));
        self.notice_box.remove_css_class("error");
        self.notice_box.remove_css_class("warning");
        self.notice_box.add_css_class(
            if !device.page.blockers.is_empty() || storage_blocks || device.sync_error.is_some() {
                "error"
            } else {
                "warning"
            },
        );
    }

    /// `MTP-43`: the preparation overview — episode titles alongside the
    /// count/size line so the user knows *what* is about to download, not
    /// just how much. Hidden entirely for `Absent`/`NothingMissing`, exactly
    /// like `device_sync_strings::preparation_overview` reports them.
    fn update_preparation(&self, device: &DeviceView) {
        let Some(summary) = device_sync_strings::preparation_overview(&device.preparation) else {
            self.preparation_box.set_visible(false);
            return;
        };
        let mut detail = summary;
        if !device.preparation_missing.is_empty() {
            let titles = device
                .preparation_missing
                .iter()
                .map(|file| file.title.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            detail = format!("{detail}\n{titles}");
        }
        self.preparation_detail.set_label(&detail);
        self.preparation_box.set_visible(true);
    }

    fn update_progress(&self, device: &DeviceView) {
        let Some((title, subtitle, speed, fraction)) = progress_copy(device) else {
            self.progress_box.set_visible(false);
            return;
        };
        self.progress_title.set_label(&title);
        self.progress_detail.set_label(&subtitle);
        self.progress_speed.set_label(&speed);
        self.progress_box.set_visible(true);
        self.progress_bar.set_fraction(fraction);
    }

    fn update_actions(&self, device: &DeviceView) {
        let copy = action_copy(device.page.controls, primary_action(&device.preparation));
        self.cancelling.set(copy.destructive);
        self.primary.set_label(copy.label);
        self.primary.set_sensitive(copy.sensitive);
        self.primary.remove_css_class("suggested-action");
        self.primary.remove_css_class("destructive-action");
        self.primary.add_css_class(if copy.destructive {
            "destructive-action"
        } else {
            "suggested-action"
        });
        self.eject.set_sensitive(eject_sensitive(device));
        self.eject
            .set_tooltip_text(Some(&device_sync_strings::eject_tooltip(
                !self.eject.is_sensitive(),
            )));
    }

    fn show_disconnected(&self) {
        if let Some(root) = self.root.upgrade() {
            root.set_visible_child_name("disconnected");
        }
    }

    #[cfg(test)]
    fn root_text(&self) -> String {
        fn append(widget: &gtk4::Widget, output: &mut String) {
            if let Ok(label) = widget.clone().downcast::<gtk4::Label>() {
                output.push_str(&label.text());
                output.push('\n');
            }
            if let Ok(button) = widget.clone().downcast::<gtk4::Button>() {
                if let Some(label) = button.label() {
                    output.push_str(&label);
                    output.push('\n');
                }
            }
            let mut child = widget.first_child();
            while let Some(current) = child {
                append(&current, output);
                child = current.next_sibling();
            }
        }
        let mut output = String::new();
        if let Some(root) = self.root.upgrade() {
            append(root.upcast_ref(), &mut output);
        }
        output
    }
}

fn page_state_callback(
    surface: Weak<DeviceSyncPage>,
    device_id: String,
) -> Rc<dyn Fn(DeviceSyncState)> {
    Rc::new(move |state| {
        let Some(surface) = surface.upgrade() else {
            return;
        };
        if let Some(device) = state.devices.iter().find(|device| device.id == device_id) {
            surface.update(device);
        } else {
            surface.show_disconnected();
        }
    })
}

pub(in crate::ui) fn open(
    content_stack: &gtk4::Stack,
    window_title: &adw::WindowTitle,
    device_id: &str,
    runtime: &Rc<DeviceSyncRuntime>,
) -> bool {
    let device = runtime
        .devices()
        .into_iter()
        .find(|device| device.id == device_id);
    let Some(device) = device else {
        return false;
    };
    let (surface, root) = DeviceSyncPage::new(
        &device,
        PageActions::for_runtime(runtime, device_id),
        &ContentPanelActions::for_runtime(runtime, device_id),
    );
    if let Some(previous) = content_stack.child_by_name("device-sync") {
        content_stack.remove(&previous);
    }
    content_stack.add_named(&root, Some("device-sync"));
    window_title.set_title(&device.name);

    let subscription = runtime.subscribe(page_state_callback(
        Rc::downgrade(&surface),
        device_id.to_string(),
    ));
    subscription.retain_for_widget(&root);
    let focus = surface
        .playlist_rows
        .borrow()
        .iter()
        .find(|row| row.button.is_sensitive())
        .map(|row| row.button.clone().upcast::<gtk4::Widget>())
        .or_else(|| {
            surface
                .primary
                .is_sensitive()
                .then(|| surface.primary.clone().upcast::<gtk4::Widget>())
        })
        .unwrap_or_else(|| surface.eject.clone().upcast::<gtk4::Widget>());
    crate::ui::window::content_stack::show_page(content_stack, "device-sync");
    gtk4::glib::idle_add_local_once(move || {
        focus.grab_focus();
    });
    true
}

#[cfg(test)]
#[path = "device_sync_page_tests.rs"]
mod tests;
