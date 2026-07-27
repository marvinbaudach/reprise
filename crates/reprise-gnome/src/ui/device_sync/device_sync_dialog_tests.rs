use std::cell::RefCell;
use std::rc::Rc;

use chrono::TimeZone;
use gtk4::prelude::*;
use libadwaita::prelude::*;
use reprise_core::device_sync::{
    DeviceSelection, DeviceSettings, DeviceStorageAccess, DeviceStorageProjection,
    DeviceStorageSnapshot, MirrorBlocker, Mp3Quality, SelectionSource, StorageComposition,
    StorageKnowledge, StorageProjectionState, SyncChangeSummary, SyncPageControls, SyncPageState,
    SyncPageWarning, SyncPlaylistRow, TransferProfile,
};

use super::*;
use crate::ui::device_sync_runtime::{DeviceView, PlannedSyncPhase, SyncFailure};

fn row() -> SyncPlaylistRow {
    SyncPlaylistRow {
        source: SelectionSource::Smart(7),
        name: Some("Road".into()),
        smart: true,
        selected: true,
        available: true,
        entry_count: 3,
        unique_track_count: 2,
        unavailable_count: 0,
        target_bytes: 32 * 1_024,
        last_synced_at: None,
    }
}

fn composition(free_bytes: Option<u64>) -> StorageComposition {
    StorageComposition {
        total_bytes: Some(128 * 1_024),
        reprise_music_bytes: 32 * 1_024,
        other_music_bytes: 16 * 1_024,
        other_used_bytes: Some(16 * 1_024),
        free_bytes,
        knowledge: StorageKnowledge::Complete,
    }
}

fn device() -> DeviceView {
    let playlist = row();
    DeviceView {
        id: "phone".into(),
        name: "Pixel 8".into(),
        icon: gtk4::gio::ThemedIcon::new("phone-symbolic").upcast(),
        connected: true,
        storage: DeviceStorageSnapshot {
            target_name: Some("Internal storage".into()),
            access: DeviceStorageAccess::Writable,
            total_bytes: Some(128 * 1_024),
            free_bytes: Some(64 * 1_024),
            reprise_music_bytes: 32 * 1_024,
            other_music_bytes: 16 * 1_024,
        },
        scan_error: None,
        settings: DeviceSettings {
            device_serial: "phone".into(),
            device_name: "Pixel 8".into(),
            selection: DeviceSelection::Sources(vec![playlist.source.clone()]),
            profile: TransferProfile::Mp3(Mp3Quality::Kbps256),
            opus_bitrate: 0,
            ratings_back: false,
            remove_deleted: false,
        },
        sync_phase: PlannedSyncPhase::Idle,
        sync_error: None::<SyncFailure>,
        last_sync: None,
        verified_managed_track_count: None,
        managed_track_count: 0,
        bytes_per_second: 0,
        page: SyncPageState {
            profile_options: TransferProfile::ALL.to_vec(),
            profile: TransferProfile::Mp3(Mp3Quality::Kbps256),
            playlists: vec![playlist],
            unique_track_count: 2,
            target_bytes: 32 * 1_024,
            changes: SyncChangeSummary {
                additions: 2,
                playlist_writes: 1,
                transfer_bytes: 32 * 1_024,
                ..Default::default()
            },
            storage: DeviceStorageProjection {
                target_name: Some("Internal storage".into()),
                access: DeviceStorageAccess::Writable,
                current: composition(Some(64 * 1_024)),
                after_sync: Some(composition(Some(48 * 1_024))),
                transfer_bytes: 32 * 1_024,
                state: StorageProjectionState::Fits,
            },
            blockers: Vec::new(),
            warnings: Vec::new(),
            controls: SyncPageControls {
                editable: true,
                can_start: true,
                can_cancel: false,
            },
        },
    }
}

#[test]
fn compact_dialog_names_each_modern_transfer_profile() {
    assert_eq!(
        TransferProfile::ALL.map(profile_label),
        [
            "Opus · 160 kbit/s (Recommended)",
            "MP3 · 256 kbit/s (Compatibility)",
            "Original files (no conversion)",
        ]
    );
}

#[test]
fn compact_dialog_playlist_copy_keeps_snapshot_repeats_and_physical_size_distinct() {
    assert_eq!(
        playlist_subtitle(&row()),
        "Smart snapshot · 3 entries · 2 unique tracks · 32.0 KiB · No verified sync time"
    );

    let mut missing = row();
    missing.available = false;
    missing.entry_count = 0;
    missing.unique_track_count = 0;
    missing.target_bytes = 0;
    assert_eq!(
        playlist_subtitle(&missing),
        "Playlist no longer exists — deselect it to continue"
    );
}

#[test]
fn mtp_12_playlist_copy_reports_its_last_verified_sync_time() {
    let mut playlist = row();
    assert_eq!(
        playlist_subtitle(&playlist),
        "Smart snapshot · 3 entries · 2 unique tracks · 32.0 KiB · No verified sync time"
    );

    playlist.last_synced_at = Some(1_753_612_496);
    let local = chrono::Local
        .timestamp_opt(1_753_612_496, 0)
        .single()
        .unwrap();
    assert_eq!(
        playlist_subtitle(&playlist),
        format!(
            "Smart snapshot · 3 entries · 2 unique tracks · 32.0 KiB · Last synced {}",
            local.format("%b %-d, %Y at %H:%M")
        )
    );
}

#[test]
fn compact_dialog_summarizes_every_mirror_change_without_paths() {
    let summary = change_summary(&SyncChangeSummary {
        additions: 2,
        replacements: 1,
        removals: 3,
        retained_unavailable: 4,
        playlist_writes: 2,
        playlist_removals: 1,
        transfer_bytes: 4 * 1_024,
    });

    assert_eq!(
        summary,
        "2 new · 1 updated · 3 removed · 4 unavailable kept · 2 playlists written · 1 playlist removed · 4.0 KiB transferred"
    );
    assert!(!summary.contains('/'));
}

#[test]
fn compact_dialog_projects_storage_blockers_and_control_mode() {
    let mut after = composition(Some(48 * 1_024));
    after.reprise_music_bytes = 48 * 1_024;
    after.other_used_bytes = Some(16 * 1_024);
    let projection = DeviceStorageProjection {
        target_name: Some("Internal storage".into()),
        access: DeviceStorageAccess::Writable,
        current: composition(Some(64 * 1_024)),
        after_sync: Some(after),
        transfer_bytes: 16 * 1_024,
        state: StorageProjectionState::Fits,
    };
    assert_eq!(
        storage_summary(&projection),
        "Writable · Music 48.0 KiB · after sync +16.0 KiB · Other 16.0 KiB · Free 48.0 KiB"
    );
    assert_eq!(
        crate::ui::device_sync::device_sync_storage_bar::segments(&projection),
        Some(
            crate::ui::device_sync::device_sync_storage_bar::StorageSegments {
                music: 48 * 1_024,
                after_sync: 16 * 1_024,
                other: 16 * 1_024,
                free: 48 * 1_024,
                total: 128 * 1_024,
            }
        )
    );
    assert_eq!(
        blocker_summary(&[MirrorBlocker::NoPlaylistsSelected]),
        Some("Select at least one playlist to synchronize.".into())
    );
    assert_eq!(
        action_copy(SyncPageControls {
            editable: false,
            can_start: false,
            can_cancel: true,
        }),
        DialogActionCopy {
            label: "_Cancel",
            sensitive: true,
            destructive: true,
        }
    );
}

#[test]
fn mtp_9_known_read_only_target_is_explicit_and_blocks_sync() {
    let mut device = device();
    device.page.storage.access = DeviceStorageAccess::ReadOnly;
    device.page.update_controls(true, true, false);

    assert_eq!(
        storage_summary(&device.page.storage),
        "Read-only · Music 48.0 KiB · after sync no change · Other 16.0 KiB · Free 48.0 KiB"
    );
    assert_eq!(
        storage_access_notice(device.page.storage.access),
        Some("The selected device storage is read-only.".into())
    );
    assert!(!device.page.controls.can_start);

    device.page.storage.access = DeviceStorageAccess::Unknown;
    assert!(storage_summary(&device.page.storage).starts_with("Write access unknown ·"));
    assert_eq!(storage_access_notice(device.page.storage.access), None);
}

#[test]
fn mtp_10_verification_summary_claims_only_post_sync_readback() {
    let mut device = device();
    assert_eq!(
        verification_summary(&device),
        "Not verified in this session"
    );

    device.sync_phase = PlannedSyncPhase::Finishing;
    assert_eq!(verification_summary(&device), "Verifying device contents…");

    device.sync_phase = PlannedSyncPhase::Idle;
    device.last_sync = Some(chrono::Utc::now());
    device.verified_managed_track_count = Some(2);
    assert_eq!(
        verification_summary(&device),
        "Verified · 2 Reprise tracks on device"
    );
}

#[test]
fn storage_segments_never_invent_unknown_capacity_or_negative_growth() {
    let mut after = composition(Some(80 * 1_024));
    after.reprise_music_bytes = 16 * 1_024;
    let mut projection = DeviceStorageProjection {
        target_name: Some("Internal storage".into()),
        access: DeviceStorageAccess::Writable,
        current: composition(Some(64 * 1_024)),
        after_sync: Some(after),
        transfer_bytes: 0,
        state: StorageProjectionState::Fits,
    };

    assert_eq!(
        crate::ui::device_sync::device_sync_storage_bar::segments(&projection),
        Some(
            crate::ui::device_sync::device_sync_storage_bar::StorageSegments {
                music: 32 * 1_024,
                after_sync: 0,
                other: 16 * 1_024,
                free: 80 * 1_024,
                total: 128 * 1_024,
            }
        )
    );
    assert_eq!(
        storage_summary(&projection),
        "Writable · Music 48.0 KiB · after sync −16.0 KiB · Other 16.0 KiB · Free 80.0 KiB"
    );

    projection.state = StorageProjectionState::CapacityUnknown;
    assert_eq!(
        crate::ui::device_sync::device_sync_storage_bar::segments(&projection),
        None
    );
    projection.state = StorageProjectionState::Fits;
    projection.after_sync.as_mut().unwrap().total_bytes = None;
    projection.after_sync.as_mut().unwrap().knowledge = StorageKnowledge::CapacityUnknown;
    assert_eq!(
        crate::ui::device_sync::device_sync_storage_bar::segments(&projection),
        None
    );

    projection.after_sync.as_mut().unwrap().total_bytes = Some(256 * 1_024);
    projection.after_sync.as_mut().unwrap().knowledge = StorageKnowledge::Complete;
    assert_eq!(
        crate::ui::device_sync::device_sync_storage_bar::segments(&projection),
        None
    );
}

#[test]
fn mtp_4_eject_is_available_only_for_an_idle_connected_device() {
    let mut device = device();
    assert!(eject_sensitive(&device));

    device.sync_phase = PlannedSyncPhase::Syncing {
        step: crate::ui::device_sync_runtime::SyncStep::Copying,
        done: 0,
        total: 1,
        current_track: "Track".into(),
        bytes_done: 0,
        bytes_total: 1,
    };
    assert!(!eject_sensitive(&device));
    device.sync_phase = PlannedSyncPhase::Finishing;
    assert!(!eject_sensitive(&device));
    device.sync_phase = PlannedSyncPhase::Idle;
    device.connected = false;
    assert!(!eject_sensitive(&device));
}

#[test]
fn compact_dialog_warning_copy_is_grammatical_and_path_free() {
    assert_eq!(
        warning_summary(&[SyncPageWarning::UnavailableNotOnDevice { track_id: 7 }]),
        ["1 track will be skipped because it is unavailable and not already on the device."]
    );
    assert_eq!(
        warning_summary(&[
            SyncPageWarning::UnavailableNotOnDevice { track_id: 7 },
            SyncPageWarning::UnavailableNotOnDevice { track_id: 8 },
            SyncPageWarning::UnsafeManagedItem,
        ]),
        [
            "2 tracks will be skipped because they are unavailable and not already on the device.",
            "An unrecognized managed item will be left untouched.",
        ]
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mtp_2_compact_dialog_renders_and_wires_only_the_playlist_mirroring_controls() {
    gtk4::init().expect("GTK test display");
    let profile_events = Rc::new(RefCell::new(Vec::new()));
    let playlist_events = Rc::new(RefCell::new(Vec::new()));
    let starts = Rc::new(RefCell::new(0));
    let cancels = Rc::new(RefCell::new(0));
    let actions = DialogActions {
        set_profile: {
            let events = profile_events.clone();
            Rc::new(move |profile| events.borrow_mut().push(profile))
        },
        set_playlist: {
            let events = playlist_events.clone();
            Rc::new(move |source, selected| events.borrow_mut().push((source, selected)))
        },
        start: {
            let starts = starts.clone();
            Rc::new(move || *starts.borrow_mut() += 1)
        },
        cancel: {
            let cancels = cancels.clone();
            Rc::new(move || *cancels.borrow_mut() += 1)
        },
        eject: Rc::new(|| {}),
    };
    let mut device = device();
    let surface = SyncDialogSurface::new(&device, actions);
    let _dialog = dialog_for_surface(&surface);

    assert_eq!(surface.title.title(), "Pixel 8");
    assert_eq!(
        surface.profile.model().map(|model| model.n_items()),
        Some(3)
    );
    assert_eq!(surface.profile.selected(), 1);
    assert_eq!(surface.playlist_rows.borrow().len(), 1);
    assert_eq!(surface.primary.label().as_deref(), Some("_Sync now"));
    assert!(surface.primary.uses_underline());
    assert!(surface.primary.is_sensitive());
    assert!(!surface.root_text().contains("Entire library"));
    assert!(!surface.root_text().contains("ratings"));
    assert!(!surface.root_text().contains("Remove unselected"));

    surface.profile.set_selected(2);
    surface.playlist_rows.borrow()[0].1.set_active(false);
    surface.primary.emit_clicked();
    assert_eq!(*profile_events.borrow(), [TransferProfile::Original]);
    assert_eq!(
        *playlist_events.borrow(),
        [(SelectionSource::Smart(7), false)]
    );
    assert_eq!(*starts.borrow(), 1);

    device.page.controls = SyncPageControls {
        editable: false,
        can_start: false,
        can_cancel: true,
    };
    surface.update(&device);
    surface.primary.emit_clicked();
    assert_eq!(surface.primary.label().as_deref(), Some("_Cancel"));
    assert!(surface.primary.has_css_class("destructive-action"));
    assert_eq!(*cancels.borrow(), 1);
}
