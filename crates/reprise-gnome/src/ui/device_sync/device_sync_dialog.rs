//! Compact per-device surface for Android playlist mirroring.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::device_sync::{
    DeviceStorageProjection, MirrorBlocker, Mp3Quality, StorageProjectionState, SyncChangeSummary,
    SyncPageControls, SyncPageWarning, SyncPlaylistRow,
};

use super::device_sync_runtime::{
    DeviceSyncRuntime, DeviceSyncState, DeviceView, PlannedSyncPhase, SyncStep,
};
use super::device_sync_strings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DialogActionCopy {
    label: &'static str,
    sensitive: bool,
    destructive: bool,
}

fn quality_label(quality: Mp3Quality) -> &'static str {
    match quality {
        Mp3Quality::Kbps128 => "128 kbit/s",
        Mp3Quality::Kbps192 => "192 kbit/s",
        Mp3Quality::Kbps256 => "256 kbit/s",
        Mp3Quality::Kbps320 => "320 kbit/s",
    }
}

fn playlist_subtitle(row: &SyncPlaylistRow) -> String {
    if !row.available {
        return "Playlist no longer exists — deselect it to continue".into();
    }
    let mut parts = Vec::new();
    if row.smart {
        parts.push("Smart snapshot".into());
    }
    parts.push(counted(row.entry_count, "entry", "entries"));
    parts.push(counted(
        row.unique_track_count,
        "unique track",
        "unique tracks",
    ));
    if row.unavailable_count > 0 {
        parts.push(counted(
            row.unavailable_count,
            "unavailable track",
            "unavailable tracks",
        ));
    }
    parts.push(device_sync_strings::file_size(row.target_bytes));
    parts.join(" · ")
}

fn change_summary(changes: &SyncChangeSummary) -> String {
    [
        counted(changes.additions, "new", "new"),
        counted(changes.replacements, "updated", "updated"),
        counted(changes.removals, "removed", "removed"),
        counted(
            changes.retained_unavailable,
            "unavailable kept",
            "unavailable kept",
        ),
        counted(
            changes.playlist_writes,
            "playlist written",
            "playlists written",
        ),
        counted(
            changes.playlist_removals,
            "playlist removed",
            "playlists removed",
        ),
        format!(
            "{} transferred",
            device_sync_strings::file_size(changes.transfer_bytes)
        ),
    ]
    .join(" · ")
}

fn storage_summary(storage: &DeviceStorageProjection) -> String {
    match storage.state {
        StorageProjectionState::Blocked => {
            "Storage projection is unavailable until the selection is valid.".into()
        }
        StorageProjectionState::Inconsistent => {
            "The device reported inconsistent storage information.".into()
        }
        StorageProjectionState::Insufficient { shortfall_bytes } => format!(
            "Not enough space · {} more needed",
            device_sync_strings::file_size(shortfall_bytes)
        ),
        StorageProjectionState::CapacityUnknown => {
            let reprise = storage
                .after_sync
                .as_ref()
                .map_or(storage.current.reprise_music_bytes, |after| {
                    after.reprise_music_bytes
                });
            format!(
                "After sync: {} Reprise · available space unknown",
                device_sync_strings::file_size(reprise)
            )
        }
        StorageProjectionState::Fits => {
            let Some(after) = &storage.after_sync else {
                return "After-sync storage is unavailable.".into();
            };
            let free = after
                .free_bytes
                .map_or_else(|| "unknown".into(), device_sync_strings::file_size);
            format!(
                "After sync: {} Reprise · {} other music · {free} free",
                device_sync_strings::file_size(after.reprise_music_bytes),
                device_sync_strings::file_size(after.other_music_bytes),
            )
        }
    }
}

fn blocker_summary(blockers: &[MirrorBlocker]) -> Option<String> {
    if blockers.is_empty() {
        return None;
    }
    if blockers
        .iter()
        .any(|blocker| blocker == &MirrorBlocker::NoPlaylistsSelected)
    {
        return Some("Select at least one playlist to synchronize.".into());
    }
    let missing = blockers
        .iter()
        .filter(|blocker| matches!(blocker, MirrorBlocker::MissingPlaylist(_)))
        .count();
    let duplicate = blockers
        .iter()
        .filter(|blocker| matches!(blocker, MirrorBlocker::DuplicatePlaylist(_)))
        .count();
    let mut parts = Vec::new();
    if missing > 0 {
        parts.push(counted(
            missing,
            "selected playlist no longer exists",
            "selected playlists no longer exist",
        ));
    }
    if duplicate > 0 {
        parts.push(counted(
            duplicate,
            "playlist is selected twice",
            "playlists are selected twice",
        ));
    }
    Some(format!("Cannot synchronize: {}.", parts.join(" · ")))
}

fn warning_summary(warnings: &[SyncPageWarning]) -> Vec<String> {
    let unavailable = warnings
        .iter()
        .filter(|warning| matches!(warning, SyncPageWarning::UnavailableNotOnDevice { .. }))
        .count();
    let mut summary = Vec::new();
    if unavailable == 1 {
        summary.push(
            "1 track will be skipped because it is unavailable and not already on the device."
                .into(),
        );
    } else if unavailable > 1 {
        summary.push(format!(
            "{unavailable} tracks will be skipped because they are unavailable and not already on the device."
        ));
    }
    if warnings.contains(&SyncPageWarning::UnsafeManagedItem) {
        summary.push("An unrecognized managed item will be left untouched.".into());
    }
    summary
}

fn action_copy(controls: SyncPageControls) -> DialogActionCopy {
    if controls.can_cancel {
        DialogActionCopy {
            label: "Cancel",
            sensitive: true,
            destructive: true,
        }
    } else {
        DialogActionCopy {
            label: "Sync now",
            sensitive: controls.can_start,
            destructive: false,
        }
    }
}

fn counted(count: usize, singular: &str, plural: &str) -> String {
    let noun = if count == 1 { singular } else { plural };
    format!("{count} {noun}")
}

#[derive(Clone)]
struct DialogActions {
    set_quality: Rc<dyn Fn(Mp3Quality)>,
    set_playlist: Rc<dyn Fn(reprise_core::device_sync::SelectionSource, bool)>,
    start: Rc<dyn Fn()>,
    cancel: Rc<dyn Fn()>,
    eject: Rc<dyn Fn()>,
}

impl DialogActions {
    fn for_runtime(runtime: &Rc<DeviceSyncRuntime>, device_id: &str) -> Self {
        let set_quality = {
            let runtime = runtime.clone();
            let device_id = device_id.to_string();
            Rc::new(move |quality| {
                if let Err(error) = runtime.set_mp3_quality(&device_id, quality) {
                    tracing::warn!(%error, "could not update Android sync quality");
                }
            }) as Rc<dyn Fn(Mp3Quality)>
        };
        let set_playlist = {
            let runtime = runtime.clone();
            let device_id = device_id.to_string();
            Rc::new(move |source, selected| {
                if let Err(error) = runtime.set_playlist_selected(&device_id, source, selected) {
                    tracing::warn!(%error, "could not update Android sync playlist");
                }
            }) as Rc<dyn Fn(reprise_core::device_sync::SelectionSource, bool)>
        };
        let start = {
            let runtime = runtime.clone();
            let device_id = device_id.to_string();
            Rc::new(move || {
                if let Err(error) = runtime.sync_now(&device_id) {
                    tracing::warn!(%error, "could not start Android synchronization");
                }
            }) as Rc<dyn Fn()>
        };
        let cancel = {
            let runtime = runtime.clone();
            let device_id = device_id.to_string();
            Rc::new(move || runtime.cancel_current(&device_id)) as Rc<dyn Fn()>
        };
        let eject = {
            let runtime = runtime.clone();
            let device_id = device_id.to_string();
            Rc::new(move || runtime.eject(&device_id)) as Rc<dyn Fn()>
        };
        Self {
            set_quality,
            set_playlist,
            start,
            cancel,
            eject,
        }
    }
}

struct SyncDialogSurface {
    root: adw::ToolbarView,
    title: adw::WindowTitle,
    connected_stack: gtk4::Stack,
    quality: adw::ComboRow,
    playlist_group: adw::PreferencesGroup,
    playlist_rows: RefCell<Vec<(reprise_core::device_sync::SelectionSource, adw::SwitchRow)>>,
    changes: adw::ActionRow,
    storage: adw::ActionRow,
    storage_bar: gtk4::ProgressBar,
    notice: adw::ActionRow,
    progress: adw::ActionRow,
    progress_bar: gtk4::ProgressBar,
    primary: gtk4::Button,
    eject: gtk4::Button,
    updating: Rc<Cell<bool>>,
    cancelling: Rc<Cell<bool>>,
    actions: DialogActions,
}

impl SyncDialogSurface {
    fn new(device: &DeviceView, actions: DialogActions) -> Self {
        let title = adw::WindowTitle::new(&device.name, "Android playlist sync");
        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&title));
        let eject = gtk4::Button::builder()
            .icon_name("media-eject-symbolic")
            .tooltip_text(device_sync_strings::eject_tooltip(false))
            .build();
        header.pack_end(&eject);

        let page = adw::PreferencesPage::new();
        page.add_css_class("reprise-device-sync-dialog");

        let format_group = adw::PreferencesGroup::builder()
            .title("Audio format")
            .description("Tracks are mirrored as broadly compatible MP3 files.")
            .build();
        let labels = Mp3Quality::ALL.map(quality_label);
        let quality_model = gtk4::StringList::new(&labels);
        let quality = adw::ComboRow::builder()
            .title("MP3 quality")
            .model(&quality_model)
            .build();
        format_group.add(&quality);
        page.add(&format_group);

        let playlist_group = adw::PreferencesGroup::builder().title("Playlists").build();
        page.add(&playlist_group);

        let summary_group = adw::PreferencesGroup::builder()
            .title("Next synchronization")
            .build();
        let changes = adw::ActionRow::builder().title("Changes").build();
        summary_group.add(&changes);
        let storage = adw::ActionRow::builder().title("Storage").build();
        summary_group.add(&storage);
        let storage_bar = gtk4::ProgressBar::new();
        storage_bar.set_show_text(false);
        storage_bar.update_property(&[gtk4::accessible::Property::Label(
            "Projected Reprise storage use",
        )]);
        summary_group.add(&storage_bar);
        page.add(&summary_group);

        let status_group = adw::PreferencesGroup::builder().title("Status").build();
        let notice = adw::ActionRow::new();
        notice.set_visible(false);
        status_group.add(&notice);
        let progress = adw::ActionRow::new();
        progress.set_visible(false);
        status_group.add(&progress);
        let progress_bar = gtk4::ProgressBar::new();
        progress_bar.set_show_text(false);
        progress_bar.set_visible(false);
        progress_bar.update_property(&[gtk4::accessible::Property::Label(
            "Synchronization progress",
        )]);
        status_group.add(&progress_bar);

        let primary = gtk4::Button::with_label("Sync now");
        primary.add_css_class("suggested-action");
        let buttons = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        buttons.set_halign(gtk4::Align::End);
        buttons.set_margin_top(6);
        buttons.append(&primary);
        status_group.add(&buttons);
        page.add(&status_group);

        let disconnected = adw::StatusPage::builder()
            .icon_name("phone-symbolic")
            .title("Device disconnected")
            .description("Reconnect the device to continue synchronization.")
            .build();
        let connected_stack = gtk4::Stack::new();
        connected_stack.add_named(&page, Some("connected"));
        connected_stack.add_named(&disconnected, Some("disconnected"));
        connected_stack.set_visible_child_name("connected");

        let root = adw::ToolbarView::new();
        root.add_top_bar(&header);
        root.set_content(Some(&connected_stack));

        let updating = Rc::new(Cell::new(false));
        {
            let updating = updating.clone();
            let set_quality = actions.set_quality.clone();
            quality.connect_selected_notify(move |row| {
                if updating.get() {
                    return;
                }
                let Some(quality) = Mp3Quality::ALL.get(row.selected() as usize).copied() else {
                    return;
                };
                set_quality(quality);
            });
        }
        let cancelling = Rc::new(Cell::new(false));
        {
            let cancelling = cancelling.clone();
            let start = actions.start.clone();
            let cancel = actions.cancel.clone();
            primary.connect_clicked(move |_| {
                if cancelling.get() {
                    cancel();
                } else {
                    start();
                }
            });
        }
        {
            let eject_action = actions.eject.clone();
            eject.connect_clicked(move |_| eject_action());
        }

        let surface = Self {
            root,
            title,
            connected_stack,
            quality,
            playlist_group,
            playlist_rows: RefCell::new(Vec::new()),
            changes,
            storage,
            storage_bar,
            notice,
            progress,
            progress_bar,
            primary,
            eject,
            updating,
            cancelling,
            actions,
        };
        surface.update(device);
        surface
    }

    fn update(&self, device: &DeviceView) {
        self.updating.set(true);
        self.title.set_title(&device.name);
        self.title.set_subtitle("Android playlist sync");
        self.connected_stack
            .set_visible_child_name(if device.connected {
                "connected"
            } else {
                "disconnected"
            });
        let selected = Mp3Quality::ALL
            .iter()
            .position(|quality| quality == &device.page.quality)
            .unwrap_or(0);
        self.quality.set_selected(selected as u32);
        self.quality.set_sensitive(device.page.controls.editable);
        self.update_playlists(device);
        self.playlist_group.set_description(Some(&format!(
            "{} · {} on device",
            counted(
                device.page.unique_track_count,
                "unique track",
                "unique tracks"
            ),
            device_sync_strings::file_size(device.page.target_bytes)
        )));
        self.changes
            .set_subtitle(&change_summary(&device.page.changes));
        self.storage.set_title(
            device
                .page
                .storage
                .target_name
                .as_deref()
                .unwrap_or("Device storage"),
        );
        self.storage
            .set_subtitle(&storage_summary(&device.page.storage));
        let storage_fraction = device.page.storage.after_sync.as_ref().and_then(|after| {
            after
                .total_bytes
                .filter(|total| *total > 0)
                .map(|total| after.reprise_music_bytes as f64 / total as f64)
        });
        self.storage_bar.set_visible(storage_fraction.is_some());
        self.storage_bar
            .set_fraction(storage_fraction.unwrap_or(0.0).clamp(0.0, 1.0));
        self.update_notice(device);
        self.update_progress(&device.sync_phase);
        self.update_actions(device);
        self.updating.set(false);
    }

    fn update_playlists(&self, device: &DeviceView) {
        let sources = device
            .page
            .playlists
            .iter()
            .map(|playlist| playlist.source.clone())
            .collect::<Vec<_>>();
        let current = self
            .playlist_rows
            .borrow()
            .iter()
            .map(|(source, _)| source.clone())
            .collect::<Vec<_>>();
        if sources != current {
            let old_rows = self
                .playlist_rows
                .borrow_mut()
                .drain(..)
                .map(|(_, row)| row)
                .collect::<Vec<_>>();
            for row in old_rows {
                self.playlist_group.remove(&row);
            }
            for playlist in &device.page.playlists {
                let row = adw::SwitchRow::new();
                let source = playlist.source.clone();
                let updating = self.updating.clone();
                let set_playlist = self.actions.set_playlist.clone();
                row.connect_active_notify(move |row| {
                    if !updating.get() {
                        set_playlist(source.clone(), row.is_active());
                    }
                });
                self.playlist_group.add(&row);
                self.playlist_rows
                    .borrow_mut()
                    .push((playlist.source.clone(), row));
            }
        }
        for ((_, row), playlist) in self
            .playlist_rows
            .borrow()
            .iter()
            .zip(&device.page.playlists)
        {
            row.set_title(playlist.name.as_deref().unwrap_or("Unavailable playlist"));
            row.set_subtitle(&playlist_subtitle(playlist));
            row.set_active(playlist.selected);
            row.set_sensitive(device.page.controls.editable);
        }
    }

    fn update_notice(&self, device: &DeviceView) {
        let mut notices = Vec::new();
        if let Some(blocker) = blocker_summary(&device.page.blockers) {
            notices.push(blocker);
        }
        notices.extend(warning_summary(&device.page.warnings));
        if let Some(error) = &device.scan_error {
            notices.push(format!("Could not inspect device storage: {error}"));
        }
        if let Some(error) = &device.sync_error {
            notices.push(error.message.clone());
        }
        self.notice.set_visible(!notices.is_empty());
        self.notice.set_title(if device.page.blockers.is_empty() {
            "Attention"
        } else {
            "Synchronization blocked"
        });
        self.notice.set_subtitle(&notices.join("\n"));
        self.notice.remove_css_class("error");
        self.notice.remove_css_class("warning");
        self.notice.add_css_class(
            if device.page.blockers.is_empty() && device.sync_error.is_none() {
                "warning"
            } else {
                "error"
            },
        );
    }

    fn update_progress(&self, phase: &PlannedSyncPhase) {
        let Some((title, subtitle, fraction)) = progress_copy(phase) else {
            self.progress.set_visible(false);
            self.progress_bar.set_visible(false);
            return;
        };
        self.progress.set_title(&title);
        self.progress.set_subtitle(&subtitle);
        self.progress.set_visible(true);
        self.progress_bar.set_fraction(fraction);
        self.progress_bar.set_visible(true);
    }

    fn update_actions(&self, device: &DeviceView) {
        let copy = action_copy(device.page.controls);
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
        self.eject.set_sensitive(
            device.connected
                && !matches!(
                    device.sync_phase,
                    PlannedSyncPhase::Syncing { .. } | PlannedSyncPhase::Finishing
                ),
        );
        self.eject
            .set_tooltip_text(Some(&device_sync_strings::eject_tooltip(
                !self.eject.is_sensitive(),
            )));
    }

    fn show_disconnected(&self) {
        self.connected_stack.set_visible_child_name("disconnected");
        self.title.set_subtitle("Disconnected");
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
        append(self.root.upcast_ref(), &mut output);
        output
    }
}

fn progress_copy(phase: &PlannedSyncPhase) -> Option<(String, String, f64)> {
    match phase {
        PlannedSyncPhase::Idle => None,
        PlannedSyncPhase::ComputingDelta => Some((
            "Checking device…".into(),
            "Reading storage and preparing the mirror plan".into(),
            0.0,
        )),
        PlannedSyncPhase::Finishing => Some((
            "Finishing synchronization…".into(),
            "Refreshing the device inventory".into(),
            1.0,
        )),
        PlannedSyncPhase::Syncing {
            step,
            done,
            total,
            current_track,
            bytes_done,
            bytes_total,
        } => {
            let step = match step {
                SyncStep::Removing => "Removing",
                SyncStep::Transcoding => "Converting",
                SyncStep::Copying => "Copying",
                SyncStep::WritingPlaylists => "Writing playlists",
            };
            let fraction = if *bytes_total > 0 {
                *bytes_done as f64 / *bytes_total as f64
            } else if *total > 0 {
                f64::from(*done) / f64::from(*total)
            } else {
                0.0
            };
            Some((
                format!("{step} · {done} of {total}"),
                current_track.clone(),
                fraction.clamp(0.0, 1.0),
            ))
        }
    }
}

fn dialog_for_surface(surface: &SyncDialogSurface) -> adw::Dialog {
    adw::Dialog::builder()
        .child(&surface.root)
        .title("Android playlist sync")
        .content_width(560)
        .content_height(660)
        .build()
}

pub(in crate::ui) fn present(
    parent: &impl IsA<gtk4::Widget>,
    device_id: &str,
    runtime: &Rc<DeviceSyncRuntime>,
) -> Option<adw::Dialog> {
    let device = runtime
        .devices()
        .into_iter()
        .find(|device| device.id == device_id)?;
    let surface = Rc::new(SyncDialogSurface::new(
        &device,
        DialogActions::for_runtime(runtime, device_id),
    ));
    let dialog = dialog_for_surface(&surface);
    let update_surface = surface.clone();
    let update_id = device_id.to_string();
    let subscription = runtime.subscribe(Rc::new(move |state: DeviceSyncState| {
        if let Some(device) = state.devices.iter().find(|device| device.id == update_id) {
            update_surface.update(device);
        } else {
            update_surface.show_disconnected();
        }
    }));
    subscription.retain_for_widget(&dialog);
    let focus = surface
        .playlist_rows
        .borrow()
        .iter()
        .find(|(_, row)| row.is_sensitive())
        .map(|(_, row)| row.clone().upcast::<gtk4::Widget>())
        .or_else(|| {
            surface
                .primary
                .is_sensitive()
                .then(|| surface.primary.clone().upcast::<gtk4::Widget>())
        })
        .unwrap_or_else(|| surface.eject.clone().upcast::<gtk4::Widget>());
    let focus_guard = crate::ui::transient_focus::TransientFocusGuard::capture(parent);
    focus_guard.bind_closable_dialog(&dialog, &focus);
    dialog.present(Some(parent));
    Some(dialog)
}

#[cfg(test)]
#[path = "device_sync_dialog_tests.rs"]
mod tests;
