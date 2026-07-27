use super::{
    project_storage, DeviceStorageAccess, DeviceStorageSnapshot, MirrorBlocker, MirrorPlan,
    StorageKnowledge, StorageProjectionState,
};

fn snapshot(
    total_bytes: Option<u64>,
    free_bytes: Option<u64>,
    reprise_music_bytes: u64,
    other_music_bytes: u64,
) -> DeviceStorageSnapshot {
    DeviceStorageSnapshot {
        target_name: Some("Internal shared storage".into()),
        total_bytes,
        free_bytes,
        reprise_music_bytes,
        other_music_bytes,
        ..DeviceStorageSnapshot::default()
    }
}

fn plan(target_bytes: u64, transfer_bytes: u64) -> MirrorPlan {
    MirrorPlan {
        target_bytes,
        transfer_bytes,
        ..MirrorPlan::default()
    }
}

#[test]
fn complete_capacity_projects_reprise_other_music_other_data_and_free_space() {
    let projection = project_storage(&snapshot(Some(1_000), Some(200), 300, 100), &plan(350, 250));

    assert_eq!(projection.current.knowledge, StorageKnowledge::Complete);
    assert_eq!(projection.current.other_used_bytes, Some(400));
    assert_eq!(projection.state, StorageProjectionState::Fits);
    assert_eq!(projection.transfer_bytes, 250);
    let after = projection.after_sync.unwrap();
    assert_eq!(after.reprise_music_bytes, 350);
    assert_eq!(after.other_music_bytes, 100);
    assert_eq!(after.other_used_bytes, Some(400));
    assert_eq!(after.free_bytes, Some(150));
}

#[test]
fn target_projection_accounts_for_removals_replacements_and_additions_as_net_reprise_usage() {
    let projection = project_storage(&snapshot(Some(2_000), Some(100), 500, 400), &plan(450, 300));

    let after = projection.after_sync.unwrap();
    assert_eq!(after.reprise_music_bytes, 450);
    assert_eq!(after.free_bytes, Some(150));
    assert_eq!(projection.transfer_bytes, 300);
}

#[test]
fn unknown_total_keeps_known_categories_and_free_projection_without_inventing_other_data() {
    let projection = project_storage(&snapshot(None, Some(500), 200, 100), &plan(350, 350));

    assert_eq!(
        projection.current.knowledge,
        StorageKnowledge::CapacityUnknown
    );
    assert_eq!(projection.current.other_used_bytes, None);
    assert_eq!(projection.state, StorageProjectionState::Fits);
    let after = projection.after_sync.unwrap();
    assert_eq!(after.total_bytes, None);
    assert_eq!(after.other_used_bytes, None);
    assert_eq!(after.free_bytes, Some(350));
}

#[test]
fn missing_free_space_keeps_the_after_sync_categories_but_not_a_fabricated_fit_result() {
    let projection = project_storage(&snapshot(Some(1_000), None, 200, 100), &plan(350, 350));

    assert_eq!(projection.state, StorageProjectionState::CapacityUnknown);
    let after = projection.after_sync.unwrap();
    assert_eq!(after.reprise_music_bytes, 350);
    assert_eq!(after.free_bytes, None);
    assert_eq!(after.other_used_bytes, None);
}

#[test]
fn contradictory_capacity_never_produces_other_data_or_a_normalized_projection() {
    let projection = project_storage(&snapshot(Some(1_000), Some(800), 250, 100), &plan(300, 50));

    assert_eq!(projection.current.knowledge, StorageKnowledge::Inconsistent);
    assert_eq!(projection.current.other_used_bytes, None);
    assert_eq!(projection.state, StorageProjectionState::Inconsistent);
    assert_eq!(projection.after_sync, None);
}

#[test]
fn insufficient_net_space_reports_the_shortfall_without_claiming_an_after_state() {
    let projection = project_storage(&snapshot(Some(1_000), Some(100), 100, 200), &plan(250, 250));

    assert_eq!(
        projection.state,
        StorageProjectionState::Insufficient {
            shortfall_bytes: 50
        }
    );
    assert_eq!(projection.after_sync, None);
}

#[test]
fn a_blocked_mirror_plan_never_projects_an_empty_device() {
    let mut blocked = plan(0, 0);
    blocked.blockers.push(MirrorBlocker::NoPlaylistsSelected);

    let projection = project_storage(&snapshot(Some(1_000), Some(100), 500, 100), &blocked);

    assert_eq!(projection.state, StorageProjectionState::Blocked);
    assert_eq!(projection.after_sync, None);
}

#[test]
fn storage_projection_preserves_the_reported_target_access() {
    let mut snapshot = snapshot(Some(1_000), Some(500), 100, 100);
    snapshot.access = DeviceStorageAccess::ReadOnly;

    let projection = project_storage(&snapshot, &plan(100, 0));

    assert_eq!(projection.access, DeviceStorageAccess::ReadOnly);
}
