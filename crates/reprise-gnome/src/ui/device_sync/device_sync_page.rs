//! Full-page per-device surface for Android playlist mirroring.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use chrono::TimeZone;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::device_sync::{
    DeviceStorageAccess, MirrorBlocker, Mp3Quality, SyncChangeSummary, SyncPageControls,
    SyncPageWarning, SyncPlaylistRow, TransferProfile,
};

use super::device_sync_runtime::{
    DeviceSyncRuntime, DeviceSyncState, DeviceView, PlannedSyncPhase, SyncStep,
};
use super::device_sync_storage_bar::StorageBar;
use super::device_sync_storage_copy::{storage_access_notice, storage_summary};
use super::device_sync_strings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PageActionCopy {
    label: &'static str,
    sensitive: bool,
    destructive: bool,
}

fn profile_label(profile: TransferProfile) -> &'static str {
    match profile {
        TransferProfile::Opus160 => "Opus · 160 kbit/s (Recommended)",
        TransferProfile::Mp3(Mp3Quality::Kbps256) => "MP3 · 256 kbit/s (Compatibility)",
        TransferProfile::Original => "Original files (no conversion)",
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
    parts.push(playlist_last_sync_copy(row.last_synced_at));
    parts.join(" · ")
}

fn playlist_last_sync_copy(last_synced_at: Option<i64>) -> String {
    let Some(last_synced_at) = last_synced_at else {
        return "No verified sync time".into();
    };
    chrono::Local
        .timestamp_opt(last_synced_at, 0)
        .single()
        .map_or_else(
            || "Verified sync time unavailable".into(),
            |timestamp| format!("Last synced {}", timestamp.format("%b %-d, %Y at %H:%M")),
        )
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

fn verification_summary(device: &DeviceView) -> String {
    if device.sync_phase == PlannedSyncPhase::Finishing {
        return "Verifying device contents…".into();
    }
    match (device.last_sync, device.verified_managed_track_count) {
        (Some(_), Some(count)) => format!(
            "Verified · {} on device",
            counted(count, "Reprise track", "Reprise tracks")
        ),
        (Some(_), None) => "Verified after synchronization".into(),
        (None, _) => "Not verified in this session".into(),
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

fn action_copy(controls: SyncPageControls) -> PageActionCopy {
    if controls.can_cancel {
        PageActionCopy {
            label: "_Cancel",
            sensitive: true,
            destructive: true,
        }
    } else {
        PageActionCopy {
            label: "_Sync now",
            sensitive: controls.can_start,
            destructive: false,
        }
    }
}

fn eject_sensitive(device: &DeviceView) -> bool {
    device.connected
        && !matches!(
            device.sync_phase,
            PlannedSyncPhase::Syncing { .. } | PlannedSyncPhase::Finishing
        )
}

fn counted(count: usize, singular: &str, plural: &str) -> String {
    let noun = if count == 1 { singular } else { plural };
    format!("{count} {noun}")
}

#[derive(Clone)]
struct PageActions {
    set_profile: Rc<dyn Fn(TransferProfile)>,
    set_playlist: Rc<dyn Fn(reprise_core::device_sync::SelectionSource, bool)>,
    start: Rc<dyn Fn()>,
    cancel: Rc<dyn Fn()>,
    eject: Rc<dyn Fn()>,
}

impl PageActions {
    fn for_runtime(runtime: &Rc<DeviceSyncRuntime>, device_id: &str) -> Self {
        let set_profile = {
            let runtime = runtime.clone();
            let device_id = device_id.to_string();
            Rc::new(move |profile| {
                if let Err(error) = runtime.set_transfer_profile(&device_id, profile) {
                    tracing::warn!(%error, "could not update Android sync transfer profile");
                }
            }) as Rc<dyn Fn(TransferProfile)>
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
            set_profile,
            set_playlist,
            start,
            cancel,
            eject,
        }
    }
}

struct DeviceSyncPage {
    root: gtk4::Stack,
    connected_stack: gtk4::Stack,
    profile: adw::ComboRow,
    playlist_group: adw::PreferencesGroup,
    playlist_rows: RefCell<Vec<(reprise_core::device_sync::SelectionSource, adw::SwitchRow)>>,
    changes: adw::ActionRow,
    storage: adw::ActionRow,
    storage_bar: StorageBar,
    verification: adw::ActionRow,
    notice: adw::ActionRow,
    progress: adw::ActionRow,
    progress_bar: gtk4::ProgressBar,
    primary: gtk4::Button,
    eject: gtk4::Button,
    updating: Rc<Cell<bool>>,
    cancelling: Rc<Cell<bool>>,
    actions: PageActions,
}

impl DeviceSyncPage {
    fn new(device: &DeviceView, actions: PageActions) -> Self {
        let eject = gtk4::Button::builder()
            .icon_name("media-eject-symbolic")
            .label("Eject")
            .tooltip_text(device_sync_strings::eject_tooltip(false))
            .build();

        let page = adw::PreferencesPage::new();
        page.add_css_class("reprise-device-sync-page");

        let format_group = adw::PreferencesGroup::builder()
            .title("Transfer")
            .description(
                "Lossless files use the selected encoder. Lossy and unknown formats are always copied unchanged.",
            )
            .build();
        let labels = TransferProfile::ALL.map(profile_label);
        let profile_model = gtk4::StringList::new(&labels);
        let profile = adw::ComboRow::builder()
            .title("Transfer profile")
            .model(&profile_model)
            .build();
        format_group.add(&profile);
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
        let storage_bar = StorageBar::new();
        summary_group.add(storage_bar.widget());
        page.add(&summary_group);

        let status_group = adw::PreferencesGroup::builder().title("Status").build();
        let verification = adw::ActionRow::builder()
            .title("Last synchronization")
            .build();
        status_group.add(&verification);
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

        let primary = gtk4::Button::with_mnemonic("_Sync now");
        primary.add_css_class("suggested-action");
        let buttons = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        buttons.set_halign(gtk4::Align::End);
        buttons.set_margin_top(6);
        buttons.append(&eject);
        buttons.append(&primary);
        status_group.add(&buttons);
        page.add(&status_group);

        let disconnected = adw::StatusPage::builder()
            .icon_name("phone-symbolic")
            .title("Device disconnected")
            .description("Reconnect the device to continue synchronization.")
            .build();
        let root = gtk4::Stack::new();
        root.add_named(&page, Some("connected"));
        root.add_named(&disconnected, Some("disconnected"));
        root.set_visible_child_name("connected");

        let updating = Rc::new(Cell::new(false));
        {
            let updating = updating.clone();
            let set_profile = actions.set_profile.clone();
            profile.connect_selected_notify(move |row| {
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
            connected_stack: root.clone(),
            root,
            profile,
            playlist_group,
            playlist_rows: RefCell::new(Vec::new()),
            changes,
            storage,
            storage_bar,
            verification,
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
        self.connected_stack
            .set_visible_child_name(if device.connected {
                "connected"
            } else {
                "disconnected"
            });
        let selected = TransferProfile::ALL
            .iter()
            .position(|profile| profile == &device.page.profile)
            .unwrap_or(0);
        self.profile.set_selected(selected as u32);
        self.profile.set_sensitive(device.page.controls.editable);
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
        self.storage_bar.update(&device.page.storage);
        self.verification
            .set_subtitle(&verification_summary(device));
        self.update_notice(device);
        self.update_progress(&device.sync_phase, device.bytes_per_second);
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
        let existing_rows = self
            .playlist_rows
            .borrow()
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let current = existing_rows
            .iter()
            .map(|(source, _)| source.clone())
            .collect::<Vec<_>>();
        if sources != current {
            let focused = existing_rows
                .iter()
                .enumerate()
                .find(|(_, (_, row))| row.is_focus() || row.has_focus())
                .map(|(index, (source, _))| (index, source.clone()));
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
                row.set_use_markup(false);
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
            if let Some((old_index, focused_source)) = focused {
                let rebuilt_rows = self
                    .playlist_rows
                    .borrow()
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>();
                let target = rebuilt_rows
                    .iter()
                    .find(|(source, _)| source == &focused_source)
                    .or_else(|| {
                        rebuilt_rows.get(old_index.min(rebuilt_rows.len().saturating_sub(1)))
                    });
                if let Some((_, row)) = target {
                    row.grab_focus();
                }
            }
        }
        let rows = self
            .playlist_rows
            .borrow()
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        for ((_, row), playlist) in rows.iter().zip(&device.page.playlists) {
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
        self.notice.set_visible(!notices.is_empty());
        let storage_blocks = device.page.storage.access == DeviceStorageAccess::ReadOnly;
        self.notice
            .set_title(if !device.page.blockers.is_empty() || storage_blocks {
                "Synchronization blocked"
            } else {
                "Attention"
            });
        self.notice.set_subtitle(&notices.join("\n"));
        self.notice.remove_css_class("error");
        self.notice.remove_css_class("warning");
        self.notice.add_css_class(
            if !device.page.blockers.is_empty() || storage_blocks || device.sync_error.is_some() {
                "error"
            } else {
                "warning"
            },
        );
    }

    fn update_progress(&self, phase: &PlannedSyncPhase, bytes_per_second: u64) {
        let Some((title, subtitle, fraction)) = progress_copy(phase, bytes_per_second) else {
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
        self.eject.set_sensitive(eject_sensitive(device));
        self.eject
            .set_tooltip_text(Some(&device_sync_strings::eject_tooltip(
                !self.eject.is_sensitive(),
            )));
    }

    fn show_disconnected(&self) {
        self.connected_stack.set_visible_child_name("disconnected");
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

fn progress_copy(phase: &PlannedSyncPhase, bytes_per_second: u64) -> Option<(String, String, f64)> {
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
            let is_copying = *step == SyncStep::Copying;
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
            let subtitle = if is_copying && bytes_per_second > 0 {
                format!(
                    "{current_track} · {}/s",
                    device_sync_strings::file_size(bytes_per_second)
                )
            } else {
                current_track.clone()
            };
            Some((
                format!("{step} · {done} of {total}"),
                subtitle,
                fraction.clamp(0.0, 1.0),
            ))
        }
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
    let surface = Rc::new(DeviceSyncPage::new(
        &device,
        PageActions::for_runtime(runtime, device_id),
    ));
    if let Some(previous) = content_stack.child_by_name("device-sync") {
        content_stack.remove(&previous);
    }
    content_stack.add_named(&surface.root, Some("device-sync"));
    window_title.set_title(&device.name);

    let subscription = runtime.subscribe(page_state_callback(
        Rc::downgrade(&surface),
        device_id.to_string(),
    ));
    subscription.retain_for_widget(&surface.root);
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
    crate::ui::window::content_stack::show_page(content_stack, "device-sync");
    gtk4::glib::idle_add_local_once(move || {
        focus.grab_focus();
    });
    true
}

#[cfg(test)]
#[path = "device_sync_page_tests.rs"]
mod tests;
