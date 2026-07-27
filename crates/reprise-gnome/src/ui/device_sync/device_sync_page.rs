//! Full-page per-device surface for Android playlist mirroring.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use chrono::TimeZone;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::device_sync::{
    DeviceStorageAccess, MirrorBlocker, Mp3Quality, SyncChangeSummary, SyncPageControls,
    SyncPageWarning, SyncPlaylistRow, TransferProfile,
};

use super::device_sync_page_layout;
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

fn device_last_sync_copy(device: &DeviceView) -> String {
    if device.sync_phase == PlannedSyncPhase::Finishing {
        return verification_summary(device);
    }
    let history = device.last_sync.as_ref().map_or_else(
        || "Never synchronized".into(),
        |timestamp| {
            format!(
                "Last synced {}",
                timestamp
                    .with_timezone(&chrono::Local)
                    .format("%b %-d, %Y at %H:%M")
            )
        },
    );
    device
        .verified_managed_track_count
        .map_or(history.clone(), |_| {
            format!("{history} · {}", verification_summary(device))
        })
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
        summary.push("An unsafe managed path will be left untouched.".into());
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
    device.page.controls.can_eject
        && device.connected
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
            Rc::new(move || match runtime.sync_now(&device_id) {
                Ok(()) => tracing::info!(device_id, "device sync started from page"),
                Err(error) => {
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
    progress_box: gtk4::Box,
    progress_title: gtk4::Label,
    progress_detail: gtk4::Label,
    progress_speed: gtk4::Label,
    progress_bar: gtk4::ProgressBar,
    primary: gtk4::Button,
    eject: gtk4::Button,
    updating: Rc<Cell<bool>>,
    cancelling: Rc<Cell<bool>>,
    actions: PageActions,
}

impl DeviceSyncPage {
    fn new(device: &DeviceView, actions: PageActions) -> (Rc<Self>, gtk4::Stack) {
        let labels = device_sync_page_layout::profile_labels(profile_label);
        let dashboard = device_sync_page_layout::build(device, &labels);
        dashboard
            .eject
            .set_tooltip_text(Some(&device_sync_strings::eject_tooltip(false)));

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
            progress_box: dashboard.progress_box,
            progress_title: dashboard.progress_title,
            progress_detail: dashboard.progress_detail,
            progress_speed: dashboard.progress_speed,
            progress_bar: dashboard.progress_bar,
            primary: dashboard.primary,
            eject: dashboard.eject,
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
        self.updating.set(true);
        self.device_name.set_label(&device.name);
        self.connection.set_label("MTP connected");
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

    fn update_progress(&self, phase: &PlannedSyncPhase, bytes_per_second: u64) {
        let Some((title, subtitle, speed, fraction)) = progress_copy(phase, bytes_per_second)
        else {
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

fn progress_copy(
    phase: &PlannedSyncPhase,
    bytes_per_second: u64,
) -> Option<(String, String, String, f64)> {
    match phase {
        PlannedSyncPhase::Idle => None,
        PlannedSyncPhase::ComputingDelta => Some((
            "Checking device…".into(),
            "Reading storage and preparing the mirror plan".into(),
            "—".into(),
            0.0,
        )),
        PlannedSyncPhase::Finishing => Some((
            "Finishing synchronization…".into(),
            "Refreshing the device inventory".into(),
            "—".into(),
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
            let speed = if is_copying && bytes_per_second > 0 {
                format!("{}/s", device_sync_strings::file_size(bytes_per_second))
            } else {
                "—".into()
            };
            Some((
                format!("{step} · {done} of {total}"),
                current_track.clone(),
                speed,
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
    let (surface, root) =
        DeviceSyncPage::new(&device, PageActions::for_runtime(runtime, device_id));
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
