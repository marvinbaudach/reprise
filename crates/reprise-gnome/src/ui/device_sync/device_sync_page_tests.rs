use std::cell::RefCell;
use std::rc::Rc;

use chrono::TimeZone;
use gtk4::prelude::*;
use reprise_core::device_sync::{
    DeviceSelection, DeviceSettings, DeviceStorageAccess, DeviceStorageProjection,
    DeviceStorageSnapshot, MirrorBlocker, Mp3Quality, SelectionSource, StorageComposition,
    StorageKnowledge, StorageProjectionState, SyncChangeSummary, SyncPageControls, SyncPageState,
    SyncPageWarning, SyncPlaylistRow, TransferProfile,
};

use super::*;
use crate::ui::device_sync_runtime::{DeviceView, PlannedSyncPhase, SyncFailure};

fn no_op_content_actions() -> ContentPanelActions {
    ContentPanelActions {
        set_target_enabled: Rc::new(|_, _| {}),
        set_target_cap: Rc::new(|_, _| {}),
        set_remove_deleted: Rc::new(|_| {}),
        set_sync_automatically: Rc::new(|_| {}),
        set_prepare_before_sync: Rc::new(|_| {}),
        scan_device: Rc::new(|| {}),
        open_folder_browser: Rc::new(|_, _| {}),
    }
}

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
            sync_automatically: true,
            prepare_before_sync: true,
        },
        sync_phase: PlannedSyncPhase::Idle,
        sync_error: None::<SyncFailure>,
        last_sync: None,
        verified_managed_track_count: None,
        managed_track_count: 0,
        bytes_per_second: 0,
        contents_state: reprise_core::device_sync::device_view::DeviceContentsState::Verified,
        content_rows: crate::ui::device_sync_runtime::empty_content_rows(),
        category_readings: crate::ui::device_sync_runtime::empty_category_readings(),
        youtube_bytes: 0,
        podcast_bytes: 0,
        youtube_selection: Default::default(),
        // `MTP-46`: these fixtures are about rendering a device that has
        // both sources in use, so both are on.
        enabled_sources: reprise_core::device_sync::podcasts::EnabledSyncSources {
            rss: true,
            youtube: true,
        },
        podcast_selection: Default::default(),
        history: Vec::new(),
        preparation: reprise_core::device_sync::PreparationPhase::Absent,
        preparation_missing: Vec::new(),
        preparation_run: crate::ui::device_sync_runtime::PreparationRunState::Idle,
        prepared_this_run: false,
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
                can_eject: true,
            },
        },
    }
}

#[test]
fn mtp_8_full_page_names_each_modern_transfer_profile() {
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
fn full_page_playlist_copy_keeps_snapshot_repeats_and_physical_size_distinct() {
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
fn mtp_15_copy_progress_separates_the_live_mtp_rate_from_track_text() {
    let phase = PlannedSyncPhase::Syncing {
        step: crate::ui::device_sync_runtime::SyncStep::Copying,
        done: 1,
        total: 2,
        current_track: "Immortal — Lorna Shore".into(),
        bytes_done: 50,
        bytes_total: 100,
    };

    assert_eq!(
        transfer_progress_copy(&phase, 2 * 1_024 * 1_024),
        Some((
            "Copying · 1 of 2".into(),
            "Immortal — Lorna Shore".into(),
            "2.0 MiB/s".into(),
            0.5,
        ))
    );
}

#[test]
fn full_page_summarizes_every_mirror_change_without_paths() {
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
fn mtp_7_full_page_projects_complete_storage_segments() {
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
        action_copy(
            SyncPageControls {
                editable: false,
                can_start: false,
                can_cancel: true,
                can_eject: false,
            },
            reprise_core::device_sync::PrimaryAction::SyncNow,
        ),
        PageActionCopy {
            label: "_Cancel",
            sensitive: true,
            destructive: true,
        }
    );
}

/// `MTP-43`: the primary button reads "Download & sync" exactly when
/// `primary_action` says `DownloadAndSync`, and only while not cancelling —
/// a cancel affordance always reads "Cancel" regardless of what would start.
#[test]
fn mtp_43_primary_button_reads_download_and_sync_only_for_that_primary_action() {
    use reprise_core::device_sync::PrimaryAction;

    let controls = SyncPageControls {
        editable: true,
        can_start: true,
        can_cancel: false,
        can_eject: true,
    };
    assert_eq!(
        action_copy(controls, PrimaryAction::DownloadAndSync),
        PageActionCopy {
            label: "_Download & sync",
            sensitive: true,
            destructive: false,
        }
    );
    assert_eq!(
        action_copy(controls, PrimaryAction::SyncNow),
        PageActionCopy {
            label: "_Sync now",
            sensitive: true,
            destructive: false,
        }
    );
    let cancelling = SyncPageControls {
        can_cancel: true,
        ..controls
    };
    assert_eq!(
        action_copy(cancelling, PrimaryAction::DownloadAndSync),
        PageActionCopy {
            label: "_Cancel",
            sensitive: true,
            destructive: true,
        },
        "a run in flight always offers Cancel, never a relabeled start action"
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
fn mtp_14_device_header_reports_the_last_device_sync_without_claiming_one() {
    let mut device = device();
    assert_eq!(device_last_sync_copy(&device), "Never synchronized");

    device.last_sync = Some(
        chrono::Utc
            .timestamp_opt(1_753_612_496, 0)
            .single()
            .unwrap(),
    );
    let local = chrono::Local
        .timestamp_opt(1_753_612_496, 0)
        .single()
        .unwrap();
    assert_eq!(
        device_last_sync_copy(&device),
        format!("Last synced {}", local.format("%b %-d, %Y at %H:%M"))
    );
}

#[test]
fn mtp_7_storage_segments_never_invent_unknown_capacity_or_negative_growth() {
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
fn full_page_warning_copy_is_grammatical_and_path_free() {
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
            "An unsafe managed path will be left untouched.",
        ]
    );
}

#[path = "device_sync_page_display_tests.rs"]
mod display_tests;
