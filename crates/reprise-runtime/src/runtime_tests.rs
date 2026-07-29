//! The properties Task 3.1 exists to establish: two clients, reconnect,
//! competing commands, event ordering and crash recovery — all without a
//! display, an audio device or a media file.

use reprise_core::device_sync::machine::Event as DeviceEvent;
use reprise_core::device_sync::{
    DesiredManagedFile, MirrorPlan, PlaylistWrite, SelectionSource, SyncTrack, TransferAction,
};
use reprise_runtime_protocol::playback::PlaybackCommand;
use reprise_runtime_protocol::queue::QueueCommand;
use reprise_runtime_protocol::ProtocolVersion;

use crate::client::{ClientHandshake, ClientId};
use crate::error::{Capability, Refused, Rejected, RuntimeError, Unavailable};
use crate::event::RuntimeEvent;
use crate::fakes::{
    FakeClock, FakeClockHandle, FakeDevices, FakeDevicesHandle, FakeLibrary, FakePlayback,
    FakePlaybackHandle,
};
use crate::ports::Ports;
use crate::runtime::{Command, DeviceCommand, Runtime};

const DEVICE: &str = "Pixel 8";

struct Harness {
    runtime: Runtime,
    playback: FakePlaybackHandle,
    devices: FakeDevicesHandle,
    clock: FakeClockHandle,
}

/// A runtime over an in-memory database, holding tracks 1..=3.
fn harness() -> Harness {
    over(reprise_core::db::open_migrated(None).expect("an in-memory database migrates"))
}

fn over(conn: rusqlite::Connection) -> Harness {
    let playback = FakePlayback::new();
    let devices = FakeDevices::new();
    let clock = FakeClock::starting_at(1_753_600_000);
    let handles = (playback.handle(), devices.handle(), clock.handle());
    let ports = Ports {
        playback: Box::new(playback),
        library: Box::new(FakeLibrary::with_tracks([1, 2, 3])),
        devices: Box::new(devices),
        clock: Box::new(clock),
    };
    Harness {
        runtime: Runtime::new(conn, ports),
        playback: handles.0,
        devices: handles.1,
        clock: handles.2,
    }
}

/// A client holding every capability — the GTK window's position.
fn full_client(runtime: &mut Runtime) -> ClientId {
    runtime
        .connect(&ClientHandshake::new([
            Capability::PlaybackControl,
            Capability::DeviceSync,
            Capability::AiCreate,
        ]))
        .expect("the current protocol version connects")
        .client
}

/// The smallest plan that still takes several steps: one playlist to write.
fn one_playlist_plan() -> MirrorPlan {
    MirrorPlan {
        playlist_writes: vec![PlaylistWrite {
            source: SelectionSource::Playlist(12),
            source_name: "Morning".into(),
            device_path: "Reprise/Morning.m3u8".into(),
            entries: Vec::new(),
            contents: "#EXTM3U\n".into(),
        }],
        ..MirrorPlan::default()
    }
}

/// A plan with one file to copy unchanged, so a run reaches the byte-moving
/// step where a transfer rate exists at all.
fn one_copy_plan(bytes: u64) -> MirrorPlan {
    let file = DesiredManagedFile {
        track: SyncTrack {
            id: 1,
            source_path: "/music/1.flac".into(),
            original_name: "1.flac".into(),
            title: "Track 1".into(),
            artist: "Artist".into(),
            album: "Album".into(),
            album_artist: "Artist".into(),
            track_number: Some(1),
            duration_ms: 180_000,
            bitrate_kbps: Some(1_000),
            size_bytes: bytes,
            source_mtime: 0,
        },
        device_path: "Reprise/1.flac".into(),
        target_bytes: bytes,
        profile_fingerprint: "original".into(),
        action: TransferAction::CopyOriginal,
    };
    MirrorPlan {
        desired_files: vec![file.clone()],
        copy: vec![file],
        transfer_bytes: bytes,
        ..MirrorPlan::default()
    }
}

fn playback_events(events: &[crate::event::SequencedEvent]) -> Vec<&RuntimeEvent> {
    events
        .iter()
        .map(|sequenced| &sequenced.event)
        .filter(|event| matches!(event, RuntimeEvent::PlaybackChanged(_)))
        .collect()
}

#[test]
fn frequent_player_reports_do_not_require_a_whole_queue_comparison() {
    use reprise_core::playback::{PlaybackState, PlayerEvent};

    for event in [
        PlayerEvent::StateChanged(PlaybackState::Playing),
        PlayerEvent::Position {
            position_ms: 30_000,
            duration_ms: 180_000,
        },
        PlayerEvent::StreamTags {
            title: Some("News".into()),
            organization: Some("Example FM".into()),
        },
    ] {
        assert!(
            !crate::runtime::player_event_can_change_queue(&event),
            "{event:?} changes playback metadata, not queue order; a 100,000-row \
             queue must not be cloned twice for a report emitted every 500 ms"
        );
    }

    for event in [
        PlayerEvent::TrackFinished,
        PlayerEvent::AdvancedToNext,
        PlayerEvent::Error("decoder stopped".into()),
    ] {
        assert!(
            crate::runtime::player_event_can_change_queue(&event),
            "{event:?} may move or unload the current queue entry"
        );
    }
}

#[test]
fn frequent_playback_commands_do_not_require_a_whole_queue_comparison() {
    use reprise_runtime_protocol::playback::PlaybackCommand;

    for command in [
        PlaybackCommand::Pause,
        PlaybackCommand::SetVolume(0.5),
        PlaybackCommand::Seek(5_000),
        PlaybackCommand::SeekTo(30_000),
        PlaybackCommand::SetRepeat("all".into()),
    ] {
        assert!(
            !crate::runtime::playback_command_can_change_queue(&command),
            "{command:?} changes playback state, not queue order; a scrubber \
             must not clone a 100,000-row queue for every target it sends"
        );
    }

    for command in [
        PlaybackCommand::Play,
        PlaybackCommand::Stop,
        PlaybackCommand::Next,
        PlaybackCommand::Previous,
        PlaybackCommand::SetShuffle(true),
    ] {
        assert!(
            crate::runtime::playback_command_can_change_queue(&command),
            "{command:?} can load, unload, move, or reorder the queue"
        );
    }
}

#[test]
fn two_clients_receive_the_same_events_under_the_same_sequence() {
    let mut harness = harness();
    let watcher = full_client(&mut harness.runtime);
    let actor = full_client(&mut harness.runtime);

    harness
        .runtime
        .command(
            actor,
            &Command::PlayTracks {
                track_ids: vec![1, 2, 3],
                start_index: 0,
            },
        )
        .expect("playing three known tracks succeeds");

    let to_actor = harness
        .runtime
        .drain(actor)
        .expect("the actor is connected");
    let to_watcher = harness
        .runtime
        .drain(watcher)
        .expect("the watcher is connected");

    assert!(!to_actor.events.is_empty(), "the command produced events");
    assert_eq!(
        to_actor.events, to_watcher.events,
        "a client that only watches learns exactly what the actor learns, \
         in the same order and under the same sequence numbers"
    );
    assert!(!to_actor.resynchronize);
}

#[test]
fn a_disconnected_client_stops_receiving_events() {
    let mut harness = harness();
    let leaving = full_client(&mut harness.runtime);
    let staying = full_client(&mut harness.runtime);

    assert!(harness.runtime.disconnect(leaving));
    harness
        .runtime
        .command(staying, &Command::Queue(QueueCommand::AddNext(vec![2])))
        .expect("queueing succeeds");

    assert!(
        !harness
            .runtime
            .drain(staying)
            .expect("still connected")
            .events
            .is_empty(),
        "the connected client still gets its events"
    );
    assert_eq!(
        harness.runtime.drain(leaving).unwrap_err(),
        RuntimeError::Unavailable(Unavailable::NotConnected),
        "a departed client has no mailbox left to drain"
    );
}

#[test]
fn a_reconnecting_client_gets_current_state_and_no_replay() {
    let mut harness = harness();
    let first = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(
            first,
            &Command::PlayTracks {
                track_ids: vec![1, 2, 3],
                start_index: 0,
            },
        )
        .expect("playing succeeds");
    harness.runtime.disconnect(first);

    // Something happens while nobody is connected.
    let interim = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(interim, &Command::Queue(QueueCommand::AddNext(vec![3])))
        .expect("queueing succeeds");
    harness.runtime.disconnect(interim);

    let reconnected = harness
        .runtime
        .connect(&ClientHandshake::new([Capability::PlaybackControl]))
        .expect("reconnecting succeeds");

    assert_eq!(
        reconnected.snapshot.playback.track_id,
        Some(1),
        "the snapshot describes the state as it is now, not as it was at the \
         first connection"
    );
    assert_eq!(reconnected.snapshot.queue.play_next_track_ids, vec![3]);
    assert!(reconnected.snapshot.queue.current_track_id.is_some(),);
    let delivery = harness
        .runtime
        .drain(reconnected.client)
        .expect("the reconnected client is connected");
    assert!(
        delivery.events.is_empty(),
        "nothing that happened while the client was away is replayed to it; \
         the snapshot already carries the result"
    );
}

#[test]
fn a_command_from_a_disconnected_client_is_refused_rather_than_buffered() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness.runtime.disconnect(client);
    harness.playback.clear();

    let error = harness
        .runtime
        .command(
            client,
            &Command::PlayTracks {
                track_ids: vec![1],
                start_index: 0,
            },
        )
        .expect_err("a departed client cannot command");

    assert_eq!(error, RuntimeError::Unavailable(Unavailable::NotConnected));
    assert!(error.is_retryable(), "reconnecting and retrying is the fix");
    assert!(
        harness.playback.calls().is_empty(),
        "the command reached no effect at all — a stale intention executed \
         later is the more dangerous failure (§9.5)"
    );
}

#[test]
fn every_delivered_event_follows_the_snapshot_it_was_taken_after() {
    let mut harness = harness();
    let connected = harness
        .runtime
        .connect(&ClientHandshake::new([Capability::PlaybackControl]))
        .expect("connecting succeeds");

    for command in [
        Command::PlayTracks {
            track_ids: vec![1, 2, 3],
            start_index: 0,
        },
        Command::Queue(QueueCommand::AddNext(vec![3])),
        Command::Playback(PlaybackCommand::SetVolume(0.5)),
        Command::Playback(PlaybackCommand::Next),
    ] {
        harness
            .runtime
            .command(connected.client, &command)
            .expect("every command in this sequence is admissible");
    }

    let delivery = harness.runtime.drain(connected.client).unwrap();
    let sequences: Vec<u64> = delivery.events.iter().map(|event| event.sequence).collect();
    assert!(
        sequences.windows(2).all(|pair| pair[0] < pair[1]),
        "sequence numbers are strictly increasing: {sequences:?}"
    );
    assert!(
        sequences
            .iter()
            .all(|sequence| *sequence > connected.snapshot.sequence),
        "every delta follows the snapshot it must be applied to, so \
         'snapshot then deltas' has no gap and no overlap: snapshot at {}, \
         events {sequences:?}",
        connected.snapshot.sequence
    );
}

#[test]
fn a_client_that_stopped_draining_is_told_to_resynchronize_rather_than_served_a_gap() {
    let mut harness = harness();
    let attentive = full_client(&mut harness.runtime);
    let distracted = full_client(&mut harness.runtime);

    // More volume steps than one mailbox holds. Each one is a real change,
    // so each publishes exactly one event.
    for step in 0..300 {
        harness
            .runtime
            .command(
                attentive,
                &Command::Playback(PlaybackCommand::SetVolume(f64::from(step % 100) / 100.0)),
            )
            .expect("setting volume succeeds");
        // The attentive client keeps up.
        harness.runtime.drain(attentive).unwrap();
    }

    let delivery = harness.runtime.drain(distracted).unwrap();
    assert!(
        delivery.resynchronize,
        "a client that fell too far behind is told to take a fresh snapshot"
    );
    assert!(
        !harness.runtime.drain(attentive).unwrap().resynchronize,
        "the client that kept draining is not"
    );
}

#[test]
fn a_missing_capability_rejects_the_command_and_changes_nothing() {
    let mut harness = harness();
    // A read-only agent: connected, but holding no mutation capability.
    let observer = harness
        .runtime
        .connect(&ClientHandshake::new([]))
        .expect("connecting succeeds")
        .client;

    let error = harness
        .runtime
        .command(
            observer,
            &Command::PlayTracks {
                track_ids: vec![1],
                start_index: 0,
            },
        )
        .expect_err("without playback:control the command is not admissible");

    assert_eq!(
        error,
        RuntimeError::Rejected(Rejected::MissingCapability(Capability::PlaybackControl))
    );
    assert_eq!(error.kind(), "rejected:missing_capability:playback:control");
    assert!(!error.is_retryable());
    assert!(
        harness.playback.calls().is_empty(),
        "the capability check happens before the effect, not after it"
    );
}

#[test]
fn a_foreign_protocol_major_is_refused_and_a_lower_minor_is_served() {
    let mut harness = harness();

    // Relative to whatever this runtime speaks, so a major bump does not
    // quietly turn "foreign" into "our own version" and leave the test
    // asserting nothing.
    let refusal = harness
        .runtime
        .connect(&ClientHandshake {
            protocol: ProtocolVersion {
                major: reprise_runtime_protocol::PROTOCOL_VERSION.major + 1,
                minor: 0,
            },
            capabilities: [Capability::PlaybackControl].into_iter().collect(),
        })
        .expect_err("a foreign major cannot decode what this runtime sends");
    assert!(matches!(
        refusal,
        RuntimeError::Refused(Refused::ProtocolMajor { .. })
    ));
    assert!(
        !refusal.is_retryable(),
        "retrying a version mismatch cannot help"
    );

    // The ordinary upgrade sequence: a client built against an older minor,
    // talking to a newer runtime. Refusing it would break every rollout.
    harness
        .runtime
        .connect(&ClientHandshake {
            protocol: ProtocolVersion {
                major: reprise_runtime_protocol::PROTOCOL_VERSION.major,
                minor: 0,
            },
            capabilities: [Capability::PlaybackControl].into_iter().collect(),
        })
        .expect("an older minor of the same major is served");
}

#[test]
fn two_clients_starting_the_same_device_start_it_once() {
    let mut harness = harness();
    let first = full_client(&mut harness.runtime);
    let second = full_client(&mut harness.runtime);

    harness
        .runtime
        .command(
            first,
            &Command::Device(DeviceCommand::Start {
                device: DEVICE.into(),
            }),
        )
        .expect("the first start begins a run");
    let loser = harness
        .runtime
        .command(
            second,
            &Command::Device(DeviceCommand::Start {
                device: DEVICE.into(),
            }),
        )
        .expect_err("the second start loses the race");

    assert_eq!(
        loser,
        RuntimeError::Rejected(Rejected::DeviceAlreadyRunning)
    );
    assert_eq!(
        harness.devices.planned(),
        vec![DEVICE.to_owned()],
        "the device was inspected once, not twice"
    );
}

#[test]
fn cancelling_a_run_nobody_started_is_rejected() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);

    assert_eq!(
        harness
            .runtime
            .command(
                client,
                &Command::Device(DeviceCommand::Cancel {
                    device: DEVICE.into(),
                }),
            )
            .expect_err("there is nothing to cancel"),
        RuntimeError::Rejected(Rejected::NoRunToCancel)
    );
}

#[test]
fn a_device_run_reports_its_phases_to_every_client() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(
            client,
            &Command::Device(DeviceCommand::Start {
                device: DEVICE.into(),
            }),
        )
        .expect("starting succeeds");
    harness.runtime.drain(client).unwrap();

    harness
        .runtime
        .on_device_plan(DEVICE, Some(one_playlist_plan()));
    // Answer every effect the machine asks for until it finishes.
    let mut guard = 0;
    loop {
        let performed = harness.devices.take_performed();
        if performed.is_empty() || guard > 16 {
            break;
        }
        guard += 1;
        for (_, effect) in performed {
            harness.runtime.on_device_event(DEVICE, answer(&effect));
        }
    }

    let snapshot = harness.runtime.snapshot().unwrap();
    let run = snapshot
        .device_runs
        .iter()
        .find(|run| run.device == DEVICE)
        .expect("the run is in the snapshot");
    assert_eq!(run.outcome.as_deref(), Some("completed"));
    assert!(
        run.failed_track_ids.is_empty(),
        "nothing failed in this run"
    );

    let phases: Vec<String> = harness
        .runtime
        .drain(client)
        .unwrap()
        .events
        .into_iter()
        .filter_map(|event| match event.event {
            RuntimeEvent::DeviceRunChanged(run) => Some(run.phase),
            _ => None,
        })
        .collect();
    assert!(
        phases.contains(&"writing_playlists".to_owned()),
        "the client saw the step the run was actually performing: {phases:?}"
    );
}

/// Answers one effect with the event that says it succeeded.
fn answer(effect: &reprise_core::device_sync::machine::Effect) -> DeviceEvent {
    use reprise_core::device_sync::machine::Effect;
    match effect {
        Effect::CleanPartials => DeviceEvent::PartialsCleaned(Ok(())),
        Effect::Transcode { .. } => DeviceEvent::Transcoded(Ok(1)),
        Effect::CopyTrack { bytes, .. } => DeviceEvent::TrackCopied(Ok(*bytes)),
        Effect::RecordFile { .. } => DeviceEvent::FileRecorded(Ok(())),
        Effect::WritePlaylist { .. } => DeviceEvent::PlaylistWritten(Ok(())),
        Effect::RecordPlaylist { .. } => DeviceEvent::PlaylistRecorded(Ok(())),
        Effect::RemoveTrack { .. } => DeviceEvent::TrackRemoved(Ok(())),
        Effect::ForgetFile { .. } => DeviceEvent::FileForgotten(Ok(())),
        Effect::RemoveReplacedFile { .. } => DeviceEvent::ReplacedFileRemoved(Ok(())),
        Effect::RemovePlaylist { .. } => DeviceEvent::PlaylistRemoved(Ok(())),
        Effect::ForgetPlaylist { .. } => DeviceEvent::PlaylistForgotten(Ok(())),
        Effect::Finished(_) => unreachable!("the runtime consumes Finished itself"),
    }
}

#[test]
fn a_runtime_over_a_crashed_predecessors_database_reads_jobs_back_and_invents_no_playback() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let database = directory.path().join("reprise.sqlite");

    let mut first =
        over(reprise_core::db::open_migrated(Some(&database)).expect("the database migrates"));
    let client = full_client(&mut first.runtime);
    first
        .runtime
        .command(
            client,
            &Command::PlayTracks {
                track_ids: vec![1, 2, 3],
                start_index: 0,
            },
        )
        .expect("playing succeeds");
    let job_id = crate::jobs::jobs_tests::enqueue_running_job(&database);
    assert!(
        !first.runtime.is_idle().unwrap(),
        "a runtime with a loaded track and a running job is not idle"
    );

    // The crash: the process goes away without unwinding anything. Dropping
    // the runtime is as much cleanup as a SIGKILL would have performed.
    drop(first);

    let mut restarted =
        over(reprise_core::db::open_migrated(Some(&database)).expect("the database reopens"));
    let reconnected = restarted
        .runtime
        .connect(&ClientHandshake::new([Capability::PlaybackControl]))
        .expect("connecting to the new runtime succeeds");

    assert_eq!(
        reconnected.snapshot.playback.status, "stopped",
        "in-memory state belonged to the dead process; the new runtime says \
         so instead of resurrecting a guess"
    );
    assert_eq!(reconnected.snapshot.playback.track_id, None);
    assert!(reconnected.snapshot.queue.context_track_ids.is_empty());
    assert!(reconnected.snapshot.device_runs.is_empty());
    assert_eq!(
        reconnected.snapshot.sequence, 0,
        "the new runtime starts its own event order rather than continuing one"
    );

    let job = reconnected
        .snapshot
        .jobs
        .iter()
        .find(|job| job.job_id == job_id)
        .expect("the job survived in SQLite, which is the half that does");
    assert_eq!(job.state, "running");
    assert!(
        restarted
            .runtime
            .drain(reconnected.client)
            .unwrap()
            .events
            .is_empty(),
        "nothing is replayed after a crash either"
    );
}

#[test]
fn a_copy_reports_a_transfer_rate_measured_against_the_runtimes_clock() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(
            client,
            &Command::Device(DeviceCommand::Start {
                device: DEVICE.into(),
            }),
        )
        .expect("starting succeeds");
    harness
        .runtime
        .on_device_plan(DEVICE, Some(one_copy_plan(4_000_000)));

    // The machine cleans partials first; answering that gets it to the copy.
    for (_, effect) in harness.devices.take_performed() {
        harness.runtime.on_device_event(DEVICE, answer(&effect));
    }
    assert_eq!(
        harness.runtime.snapshot().unwrap().device_runs[0].phase,
        "copying"
    );

    // Two seconds of wall clock for two megabytes.
    harness.clock.advance_ms(2_000);
    harness
        .runtime
        .on_device_event(DEVICE, DeviceEvent::CopyProgress { copied: 2_000_000 });

    let run = &harness.runtime.snapshot().unwrap().device_runs[0];
    assert_eq!(
        run.progress.bytes_per_second, 1_000_000,
        "the rate is derived from the injected clock, so it is measurable \
         without waiting two real seconds"
    );
    assert_eq!(run.progress.bytes_done, 2_000_000);
    assert_eq!(run.progress.bytes_total, 4_000_000);
}

#[test]
fn a_playback_failure_is_reported_as_failed_rather_than_rejected() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness.playback.refuse_playback(true);

    let error = harness
        .runtime
        .command(
            client,
            &Command::PlayTracks {
                track_ids: vec![1],
                start_index: 0,
            },
        )
        .expect_err("the backend refused");

    assert_eq!(error.category(), "failed");
    assert!(
        !error.is_retryable(),
        "the effect ran and failed; repeating it is the user's decision, not \
         the client's"
    );
    assert!(
        !error.kind().contains('/'),
        "no path reaches the client: {}",
        error.kind()
    );
}

#[test]
fn only_the_facets_that_changed_are_published() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(client, &Command::Playback(PlaybackCommand::SetVolume(0.4)))
        .expect("setting volume succeeds");
    harness.runtime.drain(client).unwrap();

    harness
        .runtime
        .command(client, &Command::Playback(PlaybackCommand::SetVolume(0.4)))
        .expect("setting the same volume again succeeds");

    let delivery = harness.runtime.drain(client).unwrap();
    assert!(
        playback_events(&delivery.events).is_empty(),
        "a command that changed nothing publishes nothing, so a client's \
         mailbox is not filled with identical snapshots"
    );
}

#[path = "runtime_attribution_tests.rs"]
mod attribution;

/// A finished-track report stamped with the stream the transport is on.
fn stamped_finished() -> reprise_core::playback::StreamEvent {
    reprise_core::playback::StreamEvent {
        generation: reprise_core::playback::StreamGeneration::from(1),
        event: reprise_core::playback::PlayerEvent::TrackFinished,
    }
}

#[path = "runtime_queue_revision_tests.rs"]
mod queue_revision;

#[path = "runtime_outcome_tests.rs"]
mod outcomes;

#[path = "runtime_gapless_tests.rs"]
mod gapless;

#[path = "runtime_effects_tests.rs"]
mod effects;

#[path = "runtime_spectrum_tests.rs"]
mod spectrum;

#[path = "runtime_paging_tests.rs"]
mod paging;

#[path = "runtime_restore_tests.rs"]
mod restore;
