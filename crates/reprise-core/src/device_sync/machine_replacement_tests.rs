use super::*;

#[test]
fn a_replaced_path_is_deleted_after_the_planned_removals_not_with_its_copy() {
    let mut plan = replacement_plan();
    plan.remove
        .push(ManagedRemoval::Inventory(existing(9, "Reprise/9.opus")));

    let (mut machine, _) = start(plan);
    machine.dispatch(Event::PartialsCleaned(Ok(())));
    machine.dispatch(copied(100, "Reprise/1.opus"));

    assert_eq!(
        machine.dispatch(Event::FileRecorded(Ok(()))),
        vec![Effect::RemoveTrack { index: 0 }],
        "the planned removals come first"
    );
    machine.dispatch(Event::TrackRemoved(Ok(())));
    assert_eq!(
        machine.dispatch(Event::FileForgotten(Ok(()))),
        vec![Effect::RemoveReplacedFile {
            device_path: "Album Artist/Album/03 Title.mp3".into(),
        }],
        "only then is the superseded path deleted"
    );
}

#[test]
fn an_adopted_copy_never_schedules_deletion_of_the_path_it_just_wrote() {
    let adopted_path = "Emmure/Speaker Of The Dead/13 song.flac";
    let mut desired = desired(1, TransferAction::CopyOriginal, 100);
    desired.device_path = "Emmure/Speaker of the Dead/13 song.flac".into();
    let mut plan = empty_plan();
    plan.replace.push(MirrorReplacement {
        existing: existing(1, adopted_path),
        desired,
    });
    plan.transfer_bytes = 100;

    let (mut machine, _) = start(plan);
    machine.dispatch(Event::PartialsCleaned(Ok(())));
    assert_eq!(
        machine.dispatch(copied(100, adopted_path)),
        vec![Effect::RecordFile {
            index: 0,
            device_size: 100,
            device_path: adopted_path.into(),
        }]
    );

    assert_eq!(
        machine.dispatch(Event::FileRecorded(Ok(()))),
        vec![Effect::Finished(SyncOutcome::Completed {
            verified_sources: Vec::new(),
        })],
        "the recorded adopted path is already the previous file and must not delete itself"
    );
}

#[test]
fn a_failed_inventory_row_fails_the_track_and_keeps_the_old_file() {
    let plan = replacement_plan();

    let (mut machine, _) = start(plan);
    machine.dispatch(Event::PartialsCleaned(Ok(())));
    machine.dispatch(copied(100, "Reprise/1.opus"));

    assert_eq!(
        machine.dispatch(Event::FileRecorded(Err("database is locked".into()))),
        vec![failed(&[1], &[])],
        "the replaced file stays until its inventory row exists"
    );
}

#[test]
fn a_failed_removal_still_lets_a_superseded_path_be_cleaned_up() {
    let mut plan = replacement_plan();
    plan.remove
        .push(ManagedRemoval::Inventory(existing(9, "Reprise/9.opus")));

    let (mut machine, _) = start(plan);
    machine.dispatch(Event::PartialsCleaned(Ok(())));
    machine.dispatch(copied(100, "Reprise/1.opus"));
    machine.dispatch(Event::FileRecorded(Ok(())));

    assert_eq!(
        machine.dispatch(Event::TrackRemoved(Err("device is busy".into()))),
        vec![Effect::RemoveReplacedFile {
            device_path: "Album Artist/Album/03 Title.mp3".into(),
        }],
        "the superseded copy is still deleted after a failed removal"
    );
}

#[test]
fn a_deferred_replacement_removal_names_its_own_removing_phase() {
    let plan = replacement_plan();
    let (mut machine, _) = start(plan);
    machine.dispatch(Event::PartialsCleaned(Ok(())));
    machine.dispatch(copied(100, "Reprise/1.opus"));
    assert_eq!(
        machine.dispatch(Event::FileRecorded(Ok(()))),
        vec![Effect::RemoveReplacedFile {
            device_path: "Album Artist/Album/03 Title.mp3".into(),
        }]
    );
    assert!(matches!(
        machine.phase(),
        PlannedSyncPhase::Syncing {
            step: SyncStep::Removing,
            current_track,
            ..
        } if current_track == "Title — Album Artist"
    ));
}
