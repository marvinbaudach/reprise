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

/// The width every presented device-page probe window is built at. Named
/// rather than repeated so a pinned probe and its siblings cannot drift apart.
const PROBE_WINDOW_WIDTH: i32 = 968;

fn no_op_content_actions() -> OnDeviceActions {
    OnDeviceActions {
        set_remove_deleted: Rc::new(|_| {}),
        set_sync_automatically: Rc::new(|_| {}),
        scan_device: Rc::new(|| {}),
        open_folder_browser: Rc::new(|_| {}),
        open_playlist_picker: Rc::new(|_| {}),
        dismiss_legacy_media_notice: Rc::new(|| {}),
        legacy_media_notice_pending: Rc::new(|| false),
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
        rememberable: true,
        memory_status: None,
        session_state: reprise_core::device_sync::DeviceSessionState::Active,
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
            remove_deleted: false,
            sync_automatically: true,
        },
        sync_phase: PlannedSyncPhase::Idle,
        sync_error: None::<SyncFailure>,
        last_sync: None,
        verified_managed_track_count: None,
        size_on_device_bytes: None,
        managed_track_count: 0,
        bytes_per_second: 0,
        contents_state: reprise_core::device_sync::device_view::DeviceContentsState::Verified,
        content_row: crate::ui::device_sync_runtime::empty_content_row(),
        target_reading: crate::ui::device_sync_runtime::empty_target_reading(),
        keep_smart_playlists_updated: true,
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
fn mtp_60_the_dock_reads_in_every_state() {
    use crate::ui::device_sync::device_sync_dock::DockReading;

    let idle = DockReading::for_device(&device());

    let mut running_device = device();
    running_device.sync_phase = PlannedSyncPhase::Syncing {
        step: crate::ui::device_sync_runtime::SyncStep::Copying,
        done: 214,
        total: 1_047,
        current_track: "Immortal — Lorna Shore".into(),
        bytes_done: 214,
        bytes_total: 1_047,
    };
    running_device.bytes_per_second = 64 * 1_024 * 1_024;
    let running = DockReading::for_device(&running_device);

    let mut finishing_device = device();
    finishing_device.sync_phase = PlannedSyncPhase::Finishing;
    let finishing = DockReading::for_device(&finishing_device);

    let mut failed_device = device();
    failed_device.sync_error = Some(SyncFailure {
        message: "The phone stopped responding.".into(),
        failed_tracks: Vec::new(),
    });
    let failed = DockReading::for_device(&failed_device);

    assert!(matches!(idle, DockReading::Idle { .. }));
    assert!(matches!(running, DockReading::Running { .. }));
    assert!(matches!(finishing, DockReading::Finishing { .. }));
    assert!(matches!(failed, DockReading::Failed { .. }));
}

#[test]
fn mtp_60_idle_dock_keeps_every_blocker_warning_and_scan_failure() {
    use crate::ui::device_sync::device_sync_dock::DockReading;

    let mut affected = device();
    affected.page.blockers = vec![MirrorBlocker::NoPlaylistsSelected];
    affected.page.storage.access = DeviceStorageAccess::ReadOnly;
    affected.page.warnings = vec![
        SyncPageWarning::UnavailableNotOnDevice { track_id: 7 },
        SyncPageWarning::UnavailableNotOnDevice { track_id: 8 },
    ];
    affected.scan_error = Some("The MTP scan stopped early.".into());

    let DockReading::Idle { summary, .. } =
        crate::ui::device_sync::device_sync_dock::DockReading::for_device(&affected)
    else {
        panic!("idle device must keep an idle dock reading");
    };
    for message in [
        "Select at least one playlist to synchronize.",
        "The selected device storage is read-only.",
        "2 tracks will be skipped because they are unavailable and not already on the device.",
        "Could not inspect device storage: The MTP scan stopped early.",
    ] {
        assert!(summary.contains(message), "missing dock message: {message}");
    }
}

#[test]
fn mtp_60_copy_progress_separates_the_live_mtp_rate_from_track_text() {
    use crate::ui::device_sync::device_sync_dock::DockReading;

    let mut copying = device();
    copying.sync_phase = PlannedSyncPhase::Syncing {
        step: crate::ui::device_sync_runtime::SyncStep::Copying,
        done: 1,
        total: 2,
        current_track: "Immortal — Lorna Shore".into(),
        bytes_done: 50,
        bytes_total: 100,
    };
    copying.bytes_per_second = 2 * 1_024 * 1_024;

    let DockReading::Running {
        current_track,
        bytes_per_second,
        ..
    } = DockReading::for_device(&copying)
    else {
        panic!("copying device must have a running dock reading");
    };
    assert_eq!(current_track.as_deref(), Some("Immortal — Lorna Shore"));
    assert_eq!(
        device_sync_strings::rate_and_remaining(bytes_per_second, None),
        "2.0 MiB/s"
    );
}

#[test]
fn mtp_24_transfer_profile_heading_names_its_music_only_scope() {
    assert_eq!(
        super::device_sync_page_layout::MUSIC_TRANSFER_PROFILE_HEADING,
        "Music transfer profile"
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
            format_local_date_time(&local)
        )
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
fn empty_full_page_change_summary_is_one_sentence() {
    assert_eq!(
        change_summary(&SyncChangeSummary::default()),
        "Nothing transferred yet."
    );
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
                this_run: 16 * 1_024,
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
        format!("Last synced {}", format_local_date_time(&local))
    );
}

#[test]
fn mtp_50_remembered_page_names_the_last_verified_size_without_a_live_diff() {
    let mut device = device();
    device.connected = false;
    device.session_state = reprise_core::device_sync::DeviceSessionState::Remembered;
    device.last_sync = Some(
        chrono::Utc
            .timestamp_opt(1_753_612_496, 0)
            .single()
            .unwrap(),
    );
    device.size_on_device_bytes = Some(2_400_000_000);

    let local = chrono::Local
        .timestamp_opt(1_753_612_496, 0)
        .single()
        .unwrap();
    assert_eq!(
        device_last_sync_copy(&device),
        format!(
            "Last synced {} · 2.2 GiB on device when last verified",
            format_local_date_time(&local)
        )
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
                this_run: 0,
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
fn mtp_61_the_storage_bar_marks_this_run_as_hatched() {
    let mut after = composition(Some(48 * 1_024));
    after.reprise_music_bytes = 48 * 1_024;
    let mut projection = DeviceStorageProjection {
        target_name: Some("Internal storage".into()),
        access: DeviceStorageAccess::Writable,
        current: composition(Some(64 * 1_024)),
        after_sync: Some(after),
        transfer_bytes: 16 * 1_024,
        state: StorageProjectionState::Fits,
    };

    let segments = crate::ui::device_sync::device_sync_storage_bar::segments(&projection)
        .expect("complete projection");
    assert_eq!(
        segments.hatched_segment_class(),
        Some("device-storage-this-run-hatched")
    );

    projection.after_sync.as_mut().unwrap().reprise_music_bytes = 32 * 1_024;
    projection.after_sync.as_mut().unwrap().free_bytes = Some(64 * 1_024);
    let segments = crate::ui::device_sync::device_sync_storage_bar::segments(&projection)
        .expect("complete projection after a deleting run");
    assert_eq!(segments.this_run, 16 * 1_024);
    assert_eq!(
        segments.hatched_segment_class(),
        Some("device-storage-this-run-hatched")
    );
}

#[test]
fn mtp_61_known_insufficient_storage_never_claims_that_space_is_unknown() {
    let mut device = device();
    device.page.storage.state = StorageProjectionState::Insufficient {
        shortfall_bytes: 32 * 1_024,
    };
    device.page.storage.after_sync = None;

    assert_eq!(
        storage_legend(&device),
        "Not enough space · 64.0 KiB free · 32.0 KiB more needed"
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

#[path = "device_sync_bars_display_tests.rs"]
mod bars_display_tests;
#[path = "device_sync_card_hierarchy_display_tests.rs"]
mod card_hierarchy_display_tests;
#[path = "device_sync_page_display_tests.rs"]
mod display_tests;
