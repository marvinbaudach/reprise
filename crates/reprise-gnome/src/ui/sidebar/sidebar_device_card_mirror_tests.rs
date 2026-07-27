use super::tests::view;
use super::*;
use reprise_core::device_sync::DeviceStorageAccess;

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
    }];
}

#[test]
fn card_detail_mode_distinguishes_delta_progress_and_synced_states() {
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

    let mut synced = view(PlannedSyncPhase::Idle);
    select_playlist(&mut synced);
    assert_eq!(detail_mode(&synced), DetailMode::Synced);
    assert_eq!(
        detail_mode(&view(PlannedSyncPhase::Idle)),
        DetailMode::Delta
    );
}

#[test]
fn independent_cards_use_their_own_mirror_delta_and_mtp_rate() {
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

    let mut idle = view(PlannedSyncPhase::Idle);
    idle.name = "Phone B".into();
    idle.storage.free_bytes = Some(2 * 1_024 * 1_024);
    select_playlist(&mut idle);
    idle.page.changes.additions = 1;
    idle.page.changes.replacements = 1;
    idle.page.changes.removals = 1;
    idle.page.changes.transfer_bytes = 1_024 * 1_024;

    assert_eq!(card_title(&copying), "Syncing Phone A");
    assert_eq!(card_subtitle(&copying), "↑ Track A · 2.0 MiB/s");
    assert_eq!(card_title(&idle), "Phone B");
    assert_eq!(
        card_subtitle(&idle),
        "3 changes · 1.0 MiB to transfer · 2.0 MiB available"
    );
}

#[test]
fn retired_entire_library_setting_is_not_a_sidebar_selection() {
    let mut device = view(PlannedSyncPhase::Idle);
    assert!(!has_mirror_selection(&device));

    select_playlist(&mut device);
    assert!(has_mirror_selection(&device));
}

#[test]
fn warnings_keep_an_idle_card_out_of_the_synced_state() {
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
fn mtp_11_idle_device_card_shows_storage_information_instead_of_a_playlist_prompt() {
    let mut device = view(PlannedSyncPhase::Idle);
    device.storage.access = DeviceStorageAccess::Writable;
    device.storage.free_bytes = Some(2 * 1_024 * 1_024);
    device
        .page
        .blockers
        .push(reprise_core::device_sync::MirrorBlocker::NoPlaylistsSelected);

    assert_eq!(card_subtitle(&device), "Writable · 2.0 MiB free");

    device.storage.access = DeviceStorageAccess::ReadOnly;
    assert_eq!(card_subtitle(&device), "Read-only · 2.0 MiB free");

    device.storage.access = DeviceStorageAccess::Unknown;
    assert_eq!(
        card_subtitle(&device),
        "Write access unknown · 2.0 MiB free"
    );
}

#[test]
fn sidebar_device_card_opens_the_compact_dialog() {
    let source = include_str!("sidebar.rs");
    assert!(source.contains("device_sync_dialog::present"));
}
