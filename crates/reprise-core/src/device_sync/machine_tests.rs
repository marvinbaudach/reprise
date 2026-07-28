//! Tests for [`DeviceSyncMachine`].
//!
//! Most of these are characterization tests: they pin the orchestration that
//! `reprise-gnome` performed before the extraction. Two behaviours are
//! deliberately *not* the old ones — the opening step label and the reach of a
//! failed transfer — and their tests say so.

use std::path::PathBuf;

use super::machine::{DeviceSyncMachine, Effect, Event, SyncOutcome, TransferSource};
use super::{
    DesiredManagedFile, DeviceFileRecord, DevicePlaylistRecord, ManagedDeviceFile, ManagedRemoval,
    MirrorPlan, MirrorReplacement, PlaylistWrite, SelectionSource, SyncTrack, TransferAction,
};
use crate::device_sync::m3u::DevicePlaylistEntry;
use crate::device_sync::PlannedSyncPhase;
use crate::device_sync::SyncStep;

const DEVICE: &str = "serial-1";

fn track(id: i64, size_bytes: u64) -> SyncTrack {
    SyncTrack {
        id,
        source_path: PathBuf::from(format!("/music/{id}.flac")),
        original_name: format!("{id}.flac"),
        title: format!("Track {id}"),
        artist: "Artist".into(),
        album: "Album".into(),
        album_artist: "Artist".into(),
        track_number: Some(1),
        duration_ms: 180_000,
        bitrate_kbps: Some(1000),
        size_bytes,
        source_mtime: 0,
    }
}

fn desired(id: i64, action: TransferAction, target_bytes: u64) -> DesiredManagedFile {
    DesiredManagedFile {
        track: track(id, target_bytes),
        device_path: format!("Reprise/{id}.opus"),
        target_bytes,
        profile_fingerprint: "fingerprint".into(),
        action,
    }
}

fn existing(id: i64, device_path: &str) -> DeviceFileRecord {
    DeviceFileRecord {
        device_serial: DEVICE.into(),
        track_id: id,
        source_path: format!("/music/{id}.flac"),
        source_size: 10,
        source_mtime: 0,
        device_path: device_path.into(),
        device_size: 10,
        profile_fingerprint: "old".into(),
        pinned: false,
    }
}

fn playlist_write(id: i64) -> PlaylistWrite {
    playlist_write_covering(id, &[])
}

/// A playlist write whose entries point at the given tracks' device paths.
fn playlist_write_covering(id: i64, track_ids: &[i64]) -> PlaylistWrite {
    PlaylistWrite {
        source: SelectionSource::Playlist(id),
        source_name: format!("Playlist {id}"),
        device_path: format!("Reprise/Playlist {id}.m3u8"),
        entries: track_ids
            .iter()
            .map(|track_id| DevicePlaylistEntry {
                relative_path: format!("Reprise/{track_id}.opus"),
                duration_secs: 180,
                display: format!("Track {track_id}"),
            })
            .collect(),
        contents: "#EXTM3U\n".into(),
    }
}

fn playlist_record(id: i64) -> DevicePlaylistRecord {
    DevicePlaylistRecord {
        device_serial: DEVICE.into(),
        source: SelectionSource::Playlist(id),
        source_name: format!("Playlist {id}"),
        device_path: format!("Reprise/Playlist {id}.m3u8"),
        last_synced_at: None,
    }
}

fn empty_plan() -> MirrorPlan {
    MirrorPlan {
        per_playlist: Vec::new(),
        desired_files: Vec::new(),
        copy: Vec::new(),
        replace: Vec::new(),
        remove: Vec::new(),
        retained_unavailable: Vec::new(),
        playlist_writes: Vec::new(),
        playlist_removals: Vec::new(),
        transfer_bytes: 0,
        target_bytes: 0,
        blockers: Vec::new(),
        warnings: Vec::new(),
    }
}

/// Drives the machine through a whole run, answering every effect with the
/// supplied outcome, and returns the effects in the order they were emitted.
fn start(plan: MirrorPlan) -> (DeviceSyncMachine, Vec<Effect>) {
    let mut machine = DeviceSyncMachine::new(DEVICE.into(), plan);
    let effects = machine.dispatch(Event::Start);
    (machine, effects)
}

#[test]
fn mtp_18_a_run_opens_on_the_step_that_actually_runs_first() {
    let mut plan = empty_plan();
    plan.copy
        .push(desired(1, TransferAction::CopyOriginal, 100));
    plan.remove
        .push(ManagedRemoval::Inventory(existing(9, "Reprise/9.opus")));
    plan.transfer_bytes = 100;

    let (machine, effects) = start(plan);

    assert_eq!(effects, vec![Effect::CleanPartials]);
    assert_eq!(
        machine.phase(),
        &PlannedSyncPhase::Syncing {
            step: SyncStep::Copying,
            done: 0,
            total: 1,
            current_track: "Track 1 — Artist".into(),
            bytes_done: 0,
            bytes_total: 100,
        },
        "the run opens on its first transfer, not on the removals that run last"
    );
}

#[test]
fn mtp_18_a_run_without_transfers_opens_on_its_playlists() {
    let mut plan = empty_plan();
    plan.playlist_writes.push(playlist_write(7));
    plan.remove
        .push(ManagedRemoval::Inventory(existing(9, "Reprise/9.opus")));

    let (machine, _) = start(plan);

    assert!(matches!(
        machine.phase(),
        PlannedSyncPhase::Syncing {
            step: SyncStep::WritingPlaylists,
            ..
        }
    ));
}

#[test]
fn mtp_18_a_run_with_nothing_but_removals_opens_on_them() {
    let mut plan = empty_plan();
    plan.remove
        .push(ManagedRemoval::Inventory(existing(9, "Reprise/9.opus")));

    let (machine, _) = start(plan);

    assert_eq!(
        machine.phase(),
        &PlannedSyncPhase::Syncing {
            step: SyncStep::Removing,
            done: 0,
            total: 1,
            current_track: "Reprise/9.opus".into(),
            bytes_done: 0,
            bytes_total: 0,
        }
    );
}

#[test]
fn a_clean_run_copies_then_writes_playlists_then_removes_then_verifies() {
    let mut plan = empty_plan();
    plan.copy
        .push(desired(1, TransferAction::CopyOriginal, 100));
    plan.playlist_writes.push(playlist_write(7));
    plan.remove
        .push(ManagedRemoval::Inventory(existing(9, "Reprise/9.opus")));
    plan.transfer_bytes = 100;

    let (mut machine, _) = start(plan);

    assert_eq!(
        machine.dispatch(Event::PartialsCleaned(Ok(()))),
        vec![Effect::CopyTrack {
            index: 0,
            source: TransferSource::Original,
            bytes: 100,
        }]
    );
    assert_eq!(
        machine.dispatch(Event::TrackCopied(Ok(100))),
        vec![Effect::RecordFile {
            index: 0,
            device_size: 100
        }]
    );
    assert_eq!(
        machine.dispatch(Event::FileRecorded(Ok(()))),
        vec![Effect::WritePlaylist { index: 0 }]
    );
    assert_eq!(
        machine.dispatch(Event::PlaylistWritten(Ok(()))),
        vec![Effect::RecordPlaylist { index: 0 }]
    );
    assert_eq!(
        machine.dispatch(Event::PlaylistRecorded(Ok(()))),
        vec![Effect::RemoveTrack { index: 0 }]
    );
    assert_eq!(
        machine.dispatch(Event::TrackRemoved(Ok(()))),
        vec![Effect::ForgetFile { index: 0 }]
    );

    let finish = machine.dispatch(Event::FileForgotten(Ok(())));
    assert_eq!(
        finish,
        vec![Effect::Finished(SyncOutcome::Completed {
            verified_sources: vec![SelectionSource::Playlist(7)],
        })]
    );
    assert_eq!(machine.phase(), &PlannedSyncPhase::Finishing);
}

#[test]
fn mtp_19_a_failed_track_suppresses_only_the_playlists_that_reference_it() {
    let mut plan = empty_plan();
    plan.copy
        .push(desired(1, TransferAction::CopyOriginal, 100));
    plan.playlist_writes.push(playlist_write_covering(7, &[1]));
    plan.playlist_writes.push(playlist_write_covering(8, &[2]));
    plan.transfer_bytes = 100;

    let (mut machine, _) = start(plan);
    machine.dispatch(Event::PartialsCleaned(Ok(())));

    assert_eq!(
        machine.dispatch(Event::TrackCopied(Err("device is full".into()))),
        vec![Effect::WritePlaylist { index: 1 }],
        "playlist 7 would point at the track that never arrived; playlist 8 would not"
    );
}

#[test]
fn mtp_19_a_failed_track_alone_does_not_block_the_removals() {
    let mut plan = empty_plan();
    plan.copy
        .push(desired(1, TransferAction::CopyOriginal, 100));
    plan.remove
        .push(ManagedRemoval::Inventory(existing(9, "Reprise/9.opus")));
    plan.transfer_bytes = 100;

    let (mut machine, _) = start(plan);
    machine.dispatch(Event::PartialsCleaned(Ok(())));

    assert_eq!(
        machine.dispatch(Event::TrackCopied(Err("device is full".into()))),
        vec![Effect::RemoveTrack { index: 0 }],
        "an obsolete file is obsolete whatever an unrelated transfer did"
    );
}

#[test]
fn mtp_19_a_playlist_held_back_by_a_failed_track_keeps_its_previous_file() {
    let mut plan = empty_plan();
    plan.copy
        .push(desired(1, TransferAction::CopyOriginal, 100));
    plan.playlist_writes.push(playlist_write_covering(7, &[1]));
    plan.playlist_removals.push(playlist_record(7));
    plan.transfer_bytes = 100;

    let (mut machine, _) = start(plan);
    machine.dispatch(Event::PartialsCleaned(Ok(())));

    assert_eq!(
        machine.dispatch(Event::TrackCopied(Err("device is full".into()))),
        vec![Effect::Finished(SyncOutcome::Failed {
            terminal_error: None,
            failed_tracks: vec![1],
        })],
        "the playlist was never rewritten, so its old file must survive"
    );
}

#[test]
fn a_failed_partial_cleanup_ends_the_run_before_any_transfer() {
    let mut plan = empty_plan();
    plan.copy
        .push(desired(1, TransferAction::CopyOriginal, 100));

    let (mut machine, _) = start(plan);

    assert_eq!(
        machine.dispatch(Event::PartialsCleaned(Err("stale handle".into()))),
        vec![Effect::Finished(SyncOutcome::Failed {
            terminal_error: Some("could not clean partial sync files: stale handle".into()),
            failed_tracks: Vec::new(),
        })]
    );
}

#[test]
fn cancelling_mid_transfer_stops_the_run_without_recording_a_failure() {
    let mut plan = empty_plan();
    plan.copy
        .push(desired(1, TransferAction::CopyOriginal, 100));
    plan.copy
        .push(desired(2, TransferAction::CopyOriginal, 100));
    plan.transfer_bytes = 200;

    let (mut machine, _) = start(plan);
    machine.dispatch(Event::PartialsCleaned(Ok(())));

    assert_eq!(machine.dispatch(Event::Cancel), Vec::new());
    assert_eq!(
        machine.dispatch(Event::TrackCopied(Err("cancelled".into()))),
        vec![Effect::Finished(SyncOutcome::Cancelled)],
        "a transfer error that arrives after a cancel is not a failure"
    );
}

#[test]
fn a_transcoded_track_is_copied_from_the_temporary_file() {
    let mut plan = empty_plan();
    plan.copy
        .push(desired(1, TransferAction::TranscodeOpus160, 100));
    plan.transfer_bytes = 100;

    let (mut machine, _) = start(plan);

    assert_eq!(
        machine.dispatch(Event::PartialsCleaned(Ok(()))),
        vec![Effect::Transcode {
            index: 0,
            action: TransferAction::TranscodeOpus160,
        }]
    );
    assert_eq!(
        machine.phase(),
        &PlannedSyncPhase::Syncing {
            step: SyncStep::Transcoding,
            done: 0,
            total: 1,
            current_track: "Track 1 — Artist".into(),
            bytes_done: 0,
            bytes_total: 100,
        }
    );
    assert_eq!(
        machine.dispatch(Event::Transcoded(Ok(64))),
        vec![Effect::CopyTrack {
            index: 0,
            source: TransferSource::Transcoded,
            bytes: 64,
        }]
    );
}

#[test]
fn a_failed_transcode_fails_its_track_without_attempting_the_copy() {
    let mut plan = empty_plan();
    plan.copy
        .push(desired(1, TransferAction::TranscodeOpus160, 100));
    plan.copy
        .push(desired(2, TransferAction::CopyOriginal, 100));
    plan.transfer_bytes = 200;

    let (mut machine, _) = start(plan);
    machine.dispatch(Event::PartialsCleaned(Ok(())));

    assert_eq!(
        machine.dispatch(Event::Transcoded(Err("no encoder".into()))),
        vec![Effect::CopyTrack {
            index: 1,
            source: TransferSource::Original,
            bytes: 100,
        }],
        "the run continues with the next transfer"
    );
    assert_eq!(machine.failed_tracks(), &[1]);
}

#[test]
fn a_replaced_path_is_deleted_after_the_planned_removals_not_with_its_copy() {
    let mut plan = empty_plan();
    plan.replace.push(MirrorReplacement {
        existing: existing(1, "Reprise/old.mp3"),
        desired: desired(1, TransferAction::CopyOriginal, 100),
    });
    plan.remove
        .push(ManagedRemoval::Inventory(existing(9, "Reprise/9.opus")));
    plan.transfer_bytes = 100;

    let (mut machine, _) = start(plan);
    machine.dispatch(Event::PartialsCleaned(Ok(())));
    machine.dispatch(Event::TrackCopied(Ok(100)));

    assert_eq!(
        machine.dispatch(Event::FileRecorded(Ok(()))),
        vec![Effect::RemoveTrack { index: 0 }],
        "the planned removals come first"
    );
    machine.dispatch(Event::TrackRemoved(Ok(())));
    assert_eq!(
        machine.dispatch(Event::FileForgotten(Ok(()))),
        vec![Effect::RemoveReplacedFile {
            device_path: "Reprise/old.mp3".into(),
        }],
        "only then is the superseded path deleted"
    );
}

#[test]
fn a_failed_inventory_row_fails_the_track_and_keeps_the_old_file() {
    let mut plan = empty_plan();
    plan.replace.push(MirrorReplacement {
        existing: existing(1, "Reprise/old.mp3"),
        desired: desired(1, TransferAction::CopyOriginal, 100),
    });
    plan.transfer_bytes = 100;

    let (mut machine, _) = start(plan);
    machine.dispatch(Event::PartialsCleaned(Ok(())));
    machine.dispatch(Event::TrackCopied(Ok(100)));

    assert_eq!(
        machine.dispatch(Event::FileRecorded(Err("database is locked".into()))),
        vec![Effect::Finished(SyncOutcome::Failed {
            terminal_error: None,
            failed_tracks: vec![1],
        })],
        "the replaced file stays until its inventory row exists"
    );
}

#[test]
fn copy_progress_advances_the_byte_counter_without_emitting_an_effect() {
    let mut plan = empty_plan();
    plan.copy
        .push(desired(1, TransferAction::CopyOriginal, 100));
    plan.copy
        .push(desired(2, TransferAction::CopyOriginal, 100));
    plan.transfer_bytes = 200;

    let (mut machine, _) = start(plan);
    machine.dispatch(Event::PartialsCleaned(Ok(())));

    assert_eq!(
        machine.dispatch(Event::CopyProgress { copied: 40 }),
        Vec::new()
    );
    let PlannedSyncPhase::Syncing { bytes_done, .. } = machine.phase() else {
        panic!("expected a syncing phase");
    };
    assert_eq!(*bytes_done, 40);
}

#[test]
fn progress_beyond_the_estimate_never_exceeds_the_planned_total() {
    let mut plan = empty_plan();
    plan.copy
        .push(desired(1, TransferAction::CopyOriginal, 100));
    plan.transfer_bytes = 100;

    let (mut machine, _) = start(plan);
    machine.dispatch(Event::PartialsCleaned(Ok(())));
    machine.dispatch(Event::CopyProgress { copied: 4_000 });

    let PlannedSyncPhase::Syncing { bytes_done, .. } = machine.phase() else {
        panic!("expected a syncing phase");
    };
    assert_eq!(*bytes_done, 100);
}

#[test]
fn a_late_duplicate_answer_cannot_advance_the_run_twice() {
    let mut plan = empty_plan();
    plan.copy
        .push(desired(1, TransferAction::CopyOriginal, 100));
    plan.copy
        .push(desired(2, TransferAction::CopyOriginal, 100));
    plan.transfer_bytes = 200;

    let (mut machine, _) = start(plan);
    machine.dispatch(Event::PartialsCleaned(Ok(())));
    machine.dispatch(Event::TrackCopied(Ok(100)));

    assert_eq!(
        machine.dispatch(Event::TrackCopied(Ok(100))),
        Vec::new(),
        "the machine is waiting for the inventory row, not for another copy"
    );
    assert_eq!(
        machine.dispatch(Event::PartialsCleaned(Ok(()))),
        Vec::new(),
        "an answer from a step already left behind is ignored"
    );
    assert_eq!(
        machine.dispatch(Event::FileRecorded(Ok(()))),
        vec![Effect::CopyTrack {
            index: 1,
            source: TransferSource::Original,
            bytes: 100,
        }]
    );
}

#[test]
fn two_devices_run_independently() {
    let mut first_plan = empty_plan();
    first_plan
        .copy
        .push(desired(1, TransferAction::CopyOriginal, 100));
    first_plan.transfer_bytes = 100;
    let mut second_plan = empty_plan();
    second_plan
        .copy
        .push(desired(2, TransferAction::CopyOriginal, 50));
    second_plan.transfer_bytes = 50;

    let mut first = DeviceSyncMachine::new("serial-1".into(), first_plan);
    let mut second = DeviceSyncMachine::new("serial-2".into(), second_plan);
    first.dispatch(Event::Start);
    second.dispatch(Event::Start);
    first.dispatch(Event::PartialsCleaned(Ok(())));
    second.dispatch(Event::PartialsCleaned(Ok(())));

    first.dispatch(Event::TrackCopied(Err("device is full".into())));

    assert_eq!(first.failed_tracks(), &[1]);
    assert_eq!(
        second.failed_tracks(),
        &[] as &[i64],
        "one device's failure does not touch the other"
    );
    assert_eq!(
        second.dispatch(Event::TrackCopied(Ok(50))),
        vec![Effect::RecordFile {
            index: 0,
            device_size: 50,
        }]
    );
}

#[test]
fn cancelling_between_playlists_stops_before_the_removals() {
    let mut plan = empty_plan();
    plan.playlist_writes.push(playlist_write(7));
    plan.playlist_writes.push(playlist_write(8));
    plan.remove
        .push(ManagedRemoval::Inventory(existing(9, "Reprise/9.opus")));

    let (mut machine, _) = start(plan);
    machine.dispatch(Event::PartialsCleaned(Ok(())));
    machine.dispatch(Event::PlaylistWritten(Ok(())));
    machine.dispatch(Event::Cancel);

    assert_eq!(
        machine.dispatch(Event::PlaylistRecorded(Ok(()))),
        vec![Effect::Finished(SyncOutcome::Cancelled)]
    );
}

#[test]
fn an_orphan_removal_has_no_inventory_row_to_forget() {
    let mut plan = empty_plan();
    plan.remove.push(ManagedRemoval::Orphan(ManagedDeviceFile {
        relative_path: "Reprise/stray.opus".into(),
        size_bytes: 10,
    }));

    let (mut machine, _) = start(plan);
    machine.dispatch(Event::PartialsCleaned(Ok(())));

    assert_eq!(
        machine.dispatch(Event::TrackRemoved(Ok(()))),
        vec![Effect::Finished(SyncOutcome::Completed {
            verified_sources: Vec::new(),
        })],
        "an orphan is deleted from the device and nowhere else"
    );
}

#[test]
fn a_still_mirrored_playlist_keeps_its_file_when_its_write_failed() {
    let mut plan = empty_plan();
    plan.playlist_writes.push(playlist_write(7));
    plan.playlist_removals.push(playlist_record(7));

    let (mut machine, _) = start(plan);
    machine.dispatch(Event::PartialsCleaned(Ok(())));

    let after_failed_write = machine.dispatch(Event::PlaylistWritten(Err("read-only".into())));

    assert_eq!(
        after_failed_write,
        vec![Effect::Finished(SyncOutcome::Failed {
            terminal_error: None,
            failed_tracks: vec![-1],
        })],
        "the stale playlist file survives a failed rewrite"
    );
}

#[test]
fn a_playlist_that_is_no_longer_mirrored_is_deleted_and_forgotten() {
    let mut plan = empty_plan();
    plan.playlist_removals.push(playlist_record(8));

    let (mut machine, _) = start(plan);

    assert_eq!(
        machine.dispatch(Event::PartialsCleaned(Ok(()))),
        vec![Effect::RemovePlaylist { index: 0 }]
    );
    assert_eq!(
        machine.dispatch(Event::PlaylistRemoved(Ok(()))),
        vec![Effect::ForgetPlaylist { index: 0 }]
    );
    assert_eq!(
        machine.dispatch(Event::PlaylistForgotten(Ok(()))),
        vec![Effect::Finished(SyncOutcome::Completed {
            verified_sources: Vec::new(),
        })]
    );
}

#[test]
fn an_empty_plan_finishes_without_touching_the_device() {
    let (mut machine, effects) = start(empty_plan());

    assert_eq!(effects, vec![Effect::CleanPartials]);
    assert_eq!(
        machine.dispatch(Event::PartialsCleaned(Ok(()))),
        vec![Effect::Finished(SyncOutcome::Completed {
            verified_sources: Vec::new(),
        })]
    );
}

#[test]
fn a_failed_removal_does_not_stop_the_removals_after_it() {
    let mut plan = empty_plan();
    plan.remove
        .push(ManagedRemoval::Inventory(existing(9, "Reprise/9.opus")));
    plan.remove
        .push(ManagedRemoval::Inventory(existing(10, "Reprise/10.opus")));

    let (mut machine, _) = start(plan);
    machine.dispatch(Event::PartialsCleaned(Ok(())));

    assert_eq!(
        machine.dispatch(Event::TrackRemoved(Err("device is busy".into()))),
        vec![Effect::RemoveTrack { index: 1 }],
        "the removal loop walks the whole plan whatever a single item did"
    );
    assert_eq!(machine.failed_tracks(), &[9]);
}

#[test]
fn a_failed_removal_still_lets_a_superseded_path_be_cleaned_up() {
    let mut plan = empty_plan();
    plan.replace.push(MirrorReplacement {
        existing: existing(1, "Reprise/old.mp3"),
        desired: desired(1, TransferAction::CopyOriginal, 100),
    });
    plan.remove
        .push(ManagedRemoval::Inventory(existing(9, "Reprise/9.opus")));
    plan.transfer_bytes = 100;

    let (mut machine, _) = start(plan);
    machine.dispatch(Event::PartialsCleaned(Ok(())));
    machine.dispatch(Event::TrackCopied(Ok(100)));
    machine.dispatch(Event::FileRecorded(Ok(())));

    assert_eq!(
        machine.dispatch(Event::TrackRemoved(Err("device is busy".into()))),
        vec![Effect::RemoveReplacedFile {
            device_path: "Reprise/old.mp3".into(),
        }],
        "the superseded copy is still deleted after a failed removal"
    );
}

#[test]
fn mtp_19_a_playlist_that_could_not_be_rewritten_holds_every_removal_back() {
    let mut plan = empty_plan();
    plan.playlist_writes.push(playlist_write(7));
    plan.remove
        .push(ManagedRemoval::Inventory(existing(9, "Reprise/9.opus")));

    let (mut machine, _) = start(plan);
    machine.dispatch(Event::PartialsCleaned(Ok(())));

    assert_eq!(
        machine.dispatch(Event::PlaylistWritten(Err("read-only".into()))),
        vec![Effect::Finished(SyncOutcome::Failed {
            terminal_error: None,
            failed_tracks: vec![-1],
        })],
        "the device still holds the old playlist, which may reference the file"
    );
}

#[test]
fn mtp_19_a_failed_transfer_that_holds_back_no_playlist_leaves_the_removals_alone() {
    let mut plan = empty_plan();
    plan.copy
        .push(desired(1, TransferAction::CopyOriginal, 100));
    plan.playlist_writes.push(playlist_write_covering(7, &[2]));
    plan.remove
        .push(ManagedRemoval::Inventory(existing(9, "Reprise/9.opus")));
    plan.transfer_bytes = 100;

    let (mut machine, _) = start(plan);
    machine.dispatch(Event::PartialsCleaned(Ok(())));
    machine.dispatch(Event::TrackCopied(Err("device is full".into())));
    machine.dispatch(Event::PlaylistWritten(Ok(())));

    assert_eq!(
        machine.dispatch(Event::PlaylistRecorded(Ok(()))),
        vec![Effect::RemoveTrack { index: 0 }],
        "every playlist was republished, so nothing stale can reference the file"
    );
}

#[test]
fn mtp_19_a_playlist_that_could_not_be_deleted_holds_every_removal_back() {
    let mut plan = empty_plan();
    plan.playlist_removals.push(playlist_record(8));
    plan.remove
        .push(ManagedRemoval::Inventory(existing(9, "Reprise/9.opus")));

    let (mut machine, _) = start(plan);
    machine.dispatch(Event::PartialsCleaned(Ok(())));

    assert_eq!(
        machine.dispatch(Event::PlaylistRemoved(Err("device is busy".into()))),
        vec![Effect::Finished(SyncOutcome::Failed {
            terminal_error: None,
            failed_tracks: vec![-1],
        })],
        "the obsolete playlist is still on the device and may name the file"
    );
}
