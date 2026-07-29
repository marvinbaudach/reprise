//! One rule per test, named for the behaviour it locks down.

use reprise_runtime_protocol::device_run::DeviceRunSnapshot;
use reprise_runtime_protocol::jobs::JobSnapshot;
use reprise_runtime_protocol::playback::{PlaybackCommand, PlaybackSnapshot};
use reprise_runtime_protocol::queue::QueueSnapshot;
use reprise_runtime_protocol::runtime::RuntimeSnapshot;

use super::RuntimeMirror;
use crate::events::{ClientError, ClientEvent, RuntimeCommand};

fn playback(status: &str) -> PlaybackSnapshot {
    PlaybackSnapshot {
        status: status.into(),
        ..Default::default()
    }
}

fn queue(current_track_id: i64) -> QueueSnapshot {
    QueueSnapshot {
        current_track_id: Some(current_track_id),
        ..Default::default()
    }
}

fn device_run(device: &str, phase: &str) -> DeviceRunSnapshot {
    DeviceRunSnapshot {
        device: device.into(),
        phase: phase.into(),
        ..Default::default()
    }
}

fn job(job_id: i64, state: &str) -> JobSnapshot {
    JobSnapshot {
        job_id,
        state: state.into(),
        ..Default::default()
    }
}

fn snapshot(sequence: u64) -> RuntimeSnapshot {
    RuntimeSnapshot {
        protocol_major: 1,
        protocol_minor: 0,
        sequence,
        client_id: 1,
        playback: playback("playing"),
        queue: queue(1),
        device_runs: vec![device_run("Pixel 8", "copying")],
        jobs: vec![job(1, "running")],
    }
}

fn connected(snapshot: RuntimeSnapshot) -> ClientEvent {
    ClientEvent::Connected(Box::new(snapshot))
}

#[test]
fn a_fresh_mirror_reports_disconnected_with_nothing_known() {
    let mirror = RuntimeMirror::new();

    assert!(
        !mirror.is_connected(),
        "before any event, there is no runtime to be connected to"
    );
    assert!(
        mirror.playback().is_none(),
        "a surface must render unavailable, not a guessed idle player"
    );
    assert!(
        mirror.queue().is_none(),
        "same as playback: unknown, not empty"
    );
    assert!(mirror.device_runs().is_empty());
    assert!(mirror.jobs().is_empty());
    assert_eq!(
        mirror.sequence(),
        0,
        "nothing has been applied yet, so there is no sequence to report"
    );
}

#[test]
fn connecting_populates_every_facet_from_the_snapshot() {
    let mut mirror = RuntimeMirror::new();

    let changed = mirror.apply(&connected(snapshot(5)));

    assert!(
        changed,
        "going from unknown to known is always a render-worthy change"
    );
    assert!(mirror.is_connected());
    assert_eq!(mirror.playback(), Some(&playback("playing")));
    assert_eq!(mirror.queue(), Some(&queue(1)));
    assert_eq!(mirror.device_runs(), &[device_run("Pixel 8", "copying")]);
    assert_eq!(mirror.jobs(), &[job(1, "running")]);
    assert_eq!(mirror.sequence(), 5);
}

#[test]
fn reconnecting_replaces_a_populated_mirror_instead_of_merging_into_it() {
    let mut mirror = RuntimeMirror::new();
    mirror.apply(&connected(snapshot(5)));
    // Layer on deltas so the mirror holds state the fresh snapshot does not
    // repeat — a second device run and a change to the one job.
    mirror.apply(&ClientEvent::DeviceRunChanged {
        sequence: 6,
        initiator: None,
        snapshot: device_run("Zune", "verifying"),
    });
    mirror.apply(&ClientEvent::JobChanged {
        sequence: 7,
        initiator: None,
        snapshot: job(1, "saved"),
    });
    assert_eq!(
        mirror.device_runs().len(),
        2,
        "setup: two device runs before reconnect"
    );

    // A fresh snapshot describing a smaller, different world entirely.
    let mut fresh = snapshot(3);
    fresh.playback = playback("stopped");
    fresh.device_runs = Vec::new();
    fresh.jobs = Vec::new();
    let changed = mirror.apply(&connected(fresh));

    assert!(changed);
    assert_eq!(
        mirror.playback(),
        Some(&playback("stopped")),
        "the new snapshot is the truth, not one input to merge with the old state"
    );
    assert!(
        mirror.device_runs().is_empty(),
        "the leftover 'Zune' run from before the reconnect must not survive a replace"
    );
    assert!(mirror.jobs().is_empty(), "same for the leftover job");
    assert_eq!(
        mirror.sequence(),
        3,
        "the snapshot's own sequence wins even though it is lower than what was held \
         before — a replace is unconditional, not itself subject to the monotonic check \
         deltas get"
    );
}

#[test]
fn disconnecting_clears_runtime_bound_state_instead_of_freezing_it() {
    let mut mirror = RuntimeMirror::new();
    mirror.apply(&connected(snapshot(5)));

    let changed = mirror.apply(&ClientEvent::Disconnected);

    assert!(changed, "known to unknown is a render-worthy change");
    assert!(!mirror.is_connected());
    assert!(
        mirror.playback().is_none(),
        "the last known playback state must not keep showing once the runtime is gone"
    );
    assert!(mirror.queue().is_none());
    assert!(
        mirror.device_runs().is_empty(),
        "device runs are runtime-bound state too, not a cache to keep across a disconnect"
    );
    assert!(mirror.jobs().is_empty());
}

#[test]
fn disconnecting_an_already_disconnected_mirror_is_a_reported_no_op() {
    let mut mirror = RuntimeMirror::new();

    let changed = mirror.apply(&ClientEvent::Disconnected);

    assert!(
        !changed,
        "nothing a surface renders differs between 'never connected' and 'disconnected again'"
    );
}

#[test]
fn a_delta_arriving_while_disconnected_is_ignored() {
    let mut mirror = RuntimeMirror::new();

    let changed = mirror.apply(&ClientEvent::PlaybackChanged {
        sequence: 1,
        initiator: None,
        snapshot: playback("playing"),
    });

    assert!(
        !changed,
        "there is no base snapshot for a delta to apply on top of"
    );
    assert!(
        mirror.playback().is_none(),
        "a delta must never be the thing that first populates playback state"
    );
    assert_eq!(
        mirror.sequence(),
        0,
        "an ignored delta must not move the sequence"
    );
}

#[test]
fn a_delta_whose_sequence_does_not_advance_is_ignored() {
    let mut mirror = RuntimeMirror::new();
    mirror.apply(&connected(snapshot(5)));

    // Equal to the current sequence: a duplicate delivery.
    let duplicate = mirror.apply(&ClientEvent::PlaybackChanged {
        sequence: 5,
        initiator: None,
        snapshot: playback("paused"),
    });
    // Less than the current sequence: delivered out of order.
    let stale = mirror.apply(&ClientEvent::PlaybackChanged {
        sequence: 2,
        initiator: None,
        snapshot: playback("stopped"),
    });

    assert!(
        !duplicate,
        "a repeat of the same sequence must not move the view"
    );
    assert!(
        !stale,
        "an older sequence arriving late must not move the view backwards"
    );
    assert_eq!(
        mirror.playback(),
        Some(&playback("playing")),
        "neither non-advancing delta may overwrite what the snapshot established"
    );
    assert_eq!(
        mirror.sequence(),
        5,
        "the sequence itself must also stay put"
    );
}

#[test]
fn a_playback_delta_with_a_greater_sequence_is_applied() {
    let mut mirror = RuntimeMirror::new();
    mirror.apply(&connected(snapshot(5)));

    let changed = mirror.apply(&ClientEvent::PlaybackChanged {
        sequence: 6,
        initiator: None,
        snapshot: playback("paused"),
    });

    assert!(changed);
    assert_eq!(mirror.playback(), Some(&playback("paused")));
    assert_eq!(mirror.sequence(), 6);
}

#[test]
fn a_queue_delta_with_a_greater_sequence_is_applied() {
    let mut mirror = RuntimeMirror::new();
    mirror.apply(&connected(snapshot(5)));

    let changed = mirror.apply(&ClientEvent::QueueChanged {
        sequence: 6,
        initiator: None,
        snapshot: queue(2),
    });

    assert!(changed);
    assert_eq!(mirror.queue(), Some(&queue(2)));
    assert_eq!(mirror.sequence(), 6);
}

#[test]
fn a_device_run_delta_updates_the_matching_device_in_place_instead_of_duplicating_it() {
    let mut mirror = RuntimeMirror::new();
    mirror.apply(&connected(snapshot(5)));
    assert_eq!(
        mirror.device_runs().len(),
        1,
        "setup: one device run from the snapshot"
    );

    let changed = mirror.apply(&ClientEvent::DeviceRunChanged {
        sequence: 6,
        initiator: None,
        snapshot: device_run("Pixel 8", "verifying"),
    });

    assert!(changed);
    assert_eq!(
        mirror.device_runs().len(),
        1,
        "the same device name must replace its row, not add a second one for it"
    );
    assert_eq!(mirror.device_runs()[0].phase, "verifying");
}

#[test]
fn a_device_run_delta_for_a_new_device_is_appended_in_sorted_order() {
    let mut mirror = RuntimeMirror::new();
    mirror.apply(&connected(snapshot(5))); // "Pixel 8"

    mirror.apply(&ClientEvent::DeviceRunChanged {
        sequence: 6,
        initiator: None,
        snapshot: device_run("Zune", "inspecting"),
    });
    mirror.apply(&ClientEvent::DeviceRunChanged {
        sequence: 7,
        initiator: None,
        snapshot: device_run("Astell&Kern", "copying"),
    });

    let names: Vec<&str> = mirror
        .device_runs()
        .iter()
        .map(|run| run.device.as_str())
        .collect();
    assert_eq!(
        names,
        vec!["Astell&Kern", "Pixel 8", "Zune"],
        "device runs stay sorted by name so a list view has a stable row order"
    );
}

#[test]
fn a_job_delta_updates_the_matching_job_in_place_instead_of_duplicating_it() {
    let mut mirror = RuntimeMirror::new();
    mirror.apply(&connected(snapshot(5)));
    assert_eq!(mirror.jobs().len(), 1, "setup: one job from the snapshot");

    let changed = mirror.apply(&ClientEvent::JobChanged {
        sequence: 6,
        initiator: None,
        snapshot: job(1, "saved"),
    });

    assert!(changed);
    assert_eq!(
        mirror.jobs().len(),
        1,
        "the same job id must replace its row, not add a second one for it"
    );
    assert_eq!(mirror.jobs()[0].state, "saved");
}

#[test]
fn a_job_delta_for_a_new_job_is_appended_in_sorted_order() {
    let mut mirror = RuntimeMirror::new();
    mirror.apply(&connected(snapshot(5))); // job_id 1

    mirror.apply(&ClientEvent::JobChanged {
        sequence: 6,
        initiator: None,
        snapshot: job(9, "running"),
    });
    mirror.apply(&ClientEvent::JobChanged {
        sequence: 7,
        initiator: None,
        snapshot: job(3, "queued"),
    });

    let ids: Vec<i64> = mirror.jobs().iter().map(|job| job.job_id).collect();
    assert_eq!(
        ids,
        vec![1, 3, 9],
        "jobs stay sorted by id so a list view has a stable row order"
    );
}

#[test]
fn command_failed_changes_no_state_and_apply_reports_no_change() {
    let mut mirror = RuntimeMirror::new();
    mirror.apply(&connected(snapshot(5)));

    let changed = mirror.apply(&ClientEvent::CommandFailed {
        command: RuntimeCommand::Playback(PlaybackCommand::Play),
        error: ClientError::Failed("playback_backend".into()),
    });

    assert!(
        !changed,
        "a command's own failure carries no runtime state for a mirror to render"
    );
    assert_eq!(
        mirror.playback(),
        Some(&playback("playing")),
        "nothing about the existing view may move because a command failed"
    );
    assert_eq!(
        mirror.sequence(),
        5,
        "a command failure carries no sequence to adopt"
    );
}

#[test]
fn reapplying_an_identical_device_run_at_a_newer_sequence_reports_no_change() {
    let mut mirror = RuntimeMirror::new();
    mirror.apply(&connected(snapshot(5)));

    let changed = mirror.apply(&ClientEvent::DeviceRunChanged {
        sequence: 6,
        initiator: None,
        snapshot: device_run("Pixel 8", "copying"),
    });

    assert!(
        !changed,
        "the sequence advanced but the row a surface would draw is byte-identical"
    );
    assert_eq!(
        mirror.sequence(),
        6,
        "the sequence still advances even when nothing renders"
    );
}

#[test]
fn a_refusal_clears_the_mirror_as_thoroughly_as_a_disconnection() {
    let mut mirror = RuntimeMirror::new();
    mirror.apply(&connected(snapshot(5)));

    let changed = mirror.apply(&ClientEvent::Refused(ClientError::Refused(
        "refused:protocol_major".into(),
    )));

    assert!(
        changed,
        "the surface has to stop showing what it was showing"
    );
    assert!(!mirror.is_connected());
    assert!(
        mirror.playback().is_none() && mirror.queue().is_none(),
        "a refusal ends the session; keeping the last values would have a \
         surface render a player it can no longer control"
    );
    assert!(mirror.device_runs().is_empty());
    assert!(mirror.jobs().is_empty());
}

#[test]
fn a_delta_after_a_refusal_is_ignored_like_any_other_disconnected_delta() {
    let mut mirror = RuntimeMirror::new();
    mirror.apply(&connected(snapshot(5)));
    mirror.apply(&ClientEvent::Refused(ClientError::Refused(
        "refused:protocol_major".into(),
    )));

    let applied = mirror.apply(&ClientEvent::PlaybackChanged {
        sequence: 9_999,
        initiator: None,
        snapshot: PlaybackSnapshot {
            status: "playing".into(),
            ..PlaybackSnapshot::default()
        },
    });

    assert!(!applied);
    assert!(
        mirror.playback().is_none(),
        "there is no base to apply it to, and inventing one would show a \
         refused client a player that is not its own"
    );
}
