use super::tests::view;
use super::*;
use reprise_core::device_sync::device_view::DeviceContentsState;
use reprise_core::device_sync::{CategoryDiff, CategoryReading, DeviceStorageAccess};

fn select_playlist(device: &mut DeviceView) {
    device.page.blockers.clear();
    device.page.playlists = vec![reprise_core::device_sync::SyncPlaylistRow {
        source: reprise_core::device_sync::SelectionSource::Playlist(1),
        name: Some("Road".into()),
        smart: false,
        selected: true,
        available: true,
        entry_count: 1,
        unique_track_count: 1,
        unavailable_count: 0,
        target_bytes: 1,
        last_synced_at: None,
    }];
}

fn diff(
    files_to_copy: usize,
    bytes_to_copy: u64,
    files_to_remove: usize,
    bytes_freed: u64,
) -> CategoryDiff {
    CategoryDiff {
        files_to_copy,
        bytes_to_copy,
        files_to_remove,
        bytes_freed,
        files_waiting_for_download: 0,
        playlists_rewritten: 0,
    }
}

#[test]
fn card_detail_mode_only_distinguishes_delta_and_progress() {
    let mut pending = view(PlannedSyncPhase::Idle);
    select_playlist(&mut pending);
    pending.page.changes.additions = 1;
    assert_eq!(detail_mode(&pending), DetailMode::Delta);

    pending.sync_phase = PlannedSyncPhase::Syncing {
        step: SyncStep::Copying,
        done: 0,
        total: 1,
        current_track: "Track".into(),
        bytes_done: 0,
        bytes_total: 1,
    };
    assert_eq!(detail_mode(&pending), DetailMode::Progress);
    assert_eq!(
        detail_mode(&view(PlannedSyncPhase::ComputingDelta)),
        DetailMode::Delta
    );
}

#[test]
fn mtp_15_sidebar_keeps_free_space_visible_during_sync() {
    let mut copying = view(PlannedSyncPhase::Syncing {
        step: SyncStep::Copying,
        done: 1,
        total: 2,
        current_track: "Track A".into(),
        bytes_done: 1,
        bytes_total: 2,
    });
    copying.name = "Phone A".into();
    copying.bytes_per_second = 2 * 1_024 * 1_024;

    copying.storage.free_bytes = Some(8 * 1_024 * 1_024);
    assert_eq!(card_title(&copying), "Syncing Phone A");
    assert_eq!(
        card_subtitle(&copying),
        "8.0 MiB free · ↑ Track A · 2.0 MiB/s"
    );
    copying.sync_phase = PlannedSyncPhase::ComputingDelta;
    assert_eq!(card_subtitle(&copying), "8.0 MiB free · Checking…");
    copying.sync_phase = PlannedSyncPhase::Finishing;
    assert_eq!(card_subtitle(&copying), "8.0 MiB free · Finishing…");
}

#[test]
fn mtp_29_idle_card_reads_the_aggregate_balance_not_a_blended_change_count() {
    let mut device = view(PlannedSyncPhase::Idle);
    select_playlist(&mut device);
    device.contents_state = DeviceContentsState::Verified;
    device.category_readings = [
        CategoryReading::Diff(diff(1, 1_024 * 1_024, 1, 0)),
        CategoryReading::SourceOff,
        CategoryReading::SourceOff,
    ];

    assert_eq!(card_title(&device), "Pixel 8");
    assert_eq!(card_subtitle(&device), "1 to copy · 1.0 MiB · 1 to remove");
}

#[test]
fn mtp_47_inert_device_card_names_the_active_device_and_offers_no_sync_copy() {
    let mut device = view(PlannedSyncPhase::Idle);
    device.session_state = reprise_core::device_sync::DeviceSessionState::Inert {
        active_device_name: "Pixel 7a (Anna)".into(),
    };

    assert_eq!(
        card_subtitle(&device),
        "Plugged in · disconnect Pixel 7a (Anna) to use it"
    );
    assert!(!device.page.controls.can_start);
}

#[test]
fn mtp_29_deletions_only_idle_card_reads_frees_not_zero_bytes() {
    let mut device = view(PlannedSyncPhase::Idle);
    device.contents_state = DeviceContentsState::Verified;
    device.category_readings = [
        CategoryReading::Diff(diff(0, 0, 3, 148 * 1_024 * 1_024)),
        CategoryReading::SourceOff,
        CategoryReading::SourceOff,
    ];

    let subtitle = card_subtitle(&device);

    assert_eq!(subtitle, "3 to remove · frees 148.0 MiB");
    assert!(!subtitle.contains("0 B"));
}

#[test]
fn mtp_29_up_to_date_idle_card_names_when_it_last_synced() {
    let mut device = view(PlannedSyncPhase::Idle);
    device.contents_state = DeviceContentsState::Verified;
    device.last_sync = Some(chrono::Utc::now() - chrono::Duration::minutes(12));

    assert_eq!(card_subtitle(&device), "Up to date · synced 12 min ago");
}

#[test]
fn mtp_29_never_verified_idle_card_prompts_a_scan_instead_of_the_balance() {
    let mut device = view(PlannedSyncPhase::Idle);
    device.contents_state = DeviceContentsState::NeverVerified;
    device.category_readings = [
        CategoryReading::Diff(diff(5, 1, 0, 0)),
        CategoryReading::SourceOff,
        CategoryReading::SourceOff,
    ];

    assert_eq!(card_subtitle(&device), "Tap to scan device contents");
}

#[test]
fn warnings_keep_an_idle_card_reading_needs_attention() {
    let mut device = view(PlannedSyncPhase::Idle);
    select_playlist(&mut device);
    device
        .page
        .warnings
        .push(reprise_core::device_sync::SyncPageWarning::UnsafeManagedItem);

    assert_eq!(detail_mode(&device), DetailMode::Delta);
    assert_eq!(
        card_subtitle(&device),
        "Needs attention · Available space unknown"
    );
}

#[test]
fn mtp_29_a_lone_no_playlists_selected_blocker_does_not_read_as_needs_attention() {
    let mut device = view(PlannedSyncPhase::Idle);
    device.storage.access = DeviceStorageAccess::Writable;
    device.storage.free_bytes = Some(2 * 1_024 * 1_024);
    device.contents_state = DeviceContentsState::Verified;
    device
        .page
        .blockers
        .push(reprise_core::device_sync::MirrorBlocker::NoPlaylistsSelected);

    assert_eq!(
        card_subtitle(&device),
        "Up to date",
        "an unselected mirror is not an error — it reads through the ordinary balance states"
    );
}

#[test]
fn mtp_13_sidebar_device_card_delegates_to_the_main_window_page() {
    let source = include_str!("sidebar.rs");
    assert!(source.contains("on_open: Rc<dyn Fn(String, String)>"));
    assert!(!source.contains("device_sync_dialog::present"));
}
