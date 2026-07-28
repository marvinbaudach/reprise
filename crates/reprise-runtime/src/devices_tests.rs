//! The device-run driver: what it forwards, what it refuses, and what it
//! reports while a run is going.

use reprise_core::device_sync::machine::{Effect, Event};
use reprise_core::device_sync::{MirrorPlan, PlaylistWrite, SelectionSource};

use super::DeviceRuns;
use crate::error::{Rejected, RuntimeError};
use crate::fakes::{FakeDevices, FakeDevicesHandle};

const DEVICE: &str = "Pixel 8";

fn plan_with_one_playlist() -> MirrorPlan {
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

struct Fixture {
    runs: DeviceRuns,
    effects: FakeDevices,
    recorded: FakeDevicesHandle,
}

fn fixture() -> Fixture {
    let effects = FakeDevices::new();
    let recorded = effects.handle();
    Fixture {
        runs: DeviceRuns::new(),
        effects,
        recorded,
    }
}

impl Fixture {
    /// Answers every outstanding effect with success until the run settles.
    fn settle(&mut self) {
        for _ in 0..16 {
            let performed = self.recorded.take_performed();
            if performed.is_empty() {
                return;
            }
            for (_, effect) in performed {
                let event = match effect {
                    Effect::CleanPartials => Event::PartialsCleaned(Ok(())),
                    Effect::WritePlaylist { .. } => Event::PlaylistWritten(Ok(())),
                    Effect::RecordPlaylist { .. } => Event::PlaylistRecorded(Ok(())),
                    other => panic!("this fixture's plan produces no {other:?}"),
                };
                self.runs.on_event(&self.effects, DEVICE, event, 0);
            }
        }
        panic!("the run did not settle");
    }
}

#[test]
fn a_started_run_asks_what_it_would_change_before_touching_anything() {
    let mut fixture = fixture();

    fixture.runs.start(&fixture.effects, DEVICE).unwrap();

    assert_eq!(fixture.recorded.planned(), vec![DEVICE.to_owned()]);
    assert!(
        fixture.recorded.performed().is_empty(),
        "no effect runs before the plan exists"
    );
    assert_eq!(
        fixture.runs.snapshot(DEVICE).unwrap().phase,
        "inspecting",
        "the run names the step it is actually performing"
    );
}

#[test]
fn a_second_start_while_one_runs_is_rejected() {
    let mut fixture = fixture();
    fixture.runs.start(&fixture.effects, DEVICE).unwrap();

    assert_eq!(
        fixture
            .runs
            .start(&fixture.effects, DEVICE)
            .expect_err("one device, one run"),
        RuntimeError::Rejected(Rejected::DeviceAlreadyRunning)
    );
    assert_eq!(fixture.recorded.planned().len(), 1);
}

#[test]
fn a_finished_run_can_be_started_again() {
    let mut fixture = fixture();
    fixture.runs.start(&fixture.effects, DEVICE).unwrap();
    fixture
        .runs
        .on_plan(&fixture.effects, DEVICE, Some(plan_with_one_playlist()), 0);
    fixture.settle();
    assert_eq!(
        fixture.runs.snapshot(DEVICE).unwrap().outcome.as_deref(),
        Some("completed")
    );

    fixture
        .runs
        .start(&fixture.effects, DEVICE)
        .expect("the device is free again");
    assert_eq!(fixture.recorded.planned().len(), 2);
    assert!(
        fixture.runs.snapshot(DEVICE).unwrap().outcome.is_none(),
        "the new run reports its own state, not the previous outcome"
    );
}

#[test]
fn a_run_that_cannot_be_planned_ends_without_having_touched_the_device() {
    let mut fixture = fixture();
    fixture.runs.start(&fixture.effects, DEVICE).unwrap();

    fixture.runs.on_plan(&fixture.effects, DEVICE, None, 0);

    let snapshot = fixture.runs.snapshot(DEVICE).unwrap();
    assert_eq!(snapshot.outcome.as_deref(), Some("failed"));
    assert_eq!(snapshot.phase, "failed");
    assert!(fixture.recorded.performed().is_empty());
    assert!(!fixture.runs.is_active());
}

#[test]
fn cancelling_before_the_plan_arrives_ends_the_run_and_ignores_the_late_plan() {
    let mut fixture = fixture();
    fixture.runs.start(&fixture.effects, DEVICE).unwrap();

    fixture.runs.cancel(&fixture.effects, DEVICE, 0).unwrap();
    assert_eq!(
        fixture.runs.snapshot(DEVICE).unwrap().outcome.as_deref(),
        Some("cancelled")
    );

    // The port was already computing when the cancel arrived; its answer
    // must not resurrect the run.
    fixture
        .runs
        .on_plan(&fixture.effects, DEVICE, Some(plan_with_one_playlist()), 0);

    assert_eq!(
        fixture.runs.snapshot(DEVICE).unwrap().outcome.as_deref(),
        Some("cancelled")
    );
    assert!(
        fixture.recorded.performed().is_empty(),
        "a cancelled run performs nothing, however late the plan is"
    );
}

#[test]
fn cancelling_a_settled_run_is_rejected() {
    let mut fixture = fixture();
    fixture.runs.start(&fixture.effects, DEVICE).unwrap();
    fixture
        .runs
        .on_plan(&fixture.effects, DEVICE, Some(plan_with_one_playlist()), 0);
    fixture.settle();

    assert_eq!(
        fixture
            .runs
            .cancel(&fixture.effects, DEVICE, 0)
            .expect_err("the run already ended"),
        RuntimeError::Rejected(Rejected::NoRunToCancel)
    );
}

#[test]
fn a_run_reports_the_step_it_is_performing() {
    let mut fixture = fixture();
    fixture.runs.start(&fixture.effects, DEVICE).unwrap();
    fixture
        .runs
        .on_plan(&fixture.effects, DEVICE, Some(plan_with_one_playlist()), 0);

    // The machine cleans partials first; the phase must already name the
    // work this run will actually do, not a step the plan does not contain.
    assert_eq!(
        fixture.runs.snapshot(DEVICE).unwrap().phase,
        "writing_playlists"
    );
    assert!(fixture.runs.is_active());
}

#[test]
fn an_unknown_device_has_no_snapshot_rather_than_an_empty_one() {
    let runs = DeviceRuns::new();

    assert!(
        runs.snapshot("never seen").is_none(),
        "an all-zero snapshot for a device nobody touched would render as \
         fact"
    );
    assert!(runs.snapshots().is_empty());
    assert!(!runs.is_active());
}

#[test]
fn events_for_a_device_that_never_ran_are_ignored() {
    let mut fixture = fixture();

    assert!(!fixture
        .runs
        .on_event(&fixture.effects, DEVICE, Event::PartialsCleaned(Ok(())), 0));
    assert!(fixture.recorded.performed().is_empty());
}
