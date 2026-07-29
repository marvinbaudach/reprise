//! Device runs: one [`DeviceSyncMachine`] per device, plus the loop that
//! keeps it fed.
//!
//! Task 2.1 made the run a pure reducer and Task 2.2 gave the Linux effects
//! their own home; what remained in the GTK crate was the driver that sits
//! between them — dispatch an event, hand the resulting effects to whoever
//! performs them, project a phase for rendering. That driver is here now, so
//! a run survives a closed window and an agent can watch one without a
//! display.

use std::collections::BTreeMap;

use reprise_core::device_sync::machine::{
    DeviceSyncMachine, Effect, Event, PlannedSyncPhase, SyncOutcome, SyncStep,
};
use reprise_core::device_sync::MirrorPlan;
use reprise_runtime_protocol::device_run::DeviceRunSnapshot;
use reprise_runtime_protocol::device_sync::DeviceProgress;

use crate::error::{Rejected, RuntimeError};
use crate::ports::DeviceEffects;

/// Shortest interval between two rate samples. Below this, a single fast
/// chunk would swing the reported rate wildly; the number is a display
/// smoothing constant, not a throughput limit.
const RATE_SAMPLE_INTERVAL_MS: u64 = 250;

/// Transfer rate, derived from copy progress and the injected clock.
#[derive(Default)]
struct RateMeter {
    bytes_per_second: u64,
    sample: Option<(u64, u64)>,
}

impl RateMeter {
    /// Starts a fresh measurement. Copy progress restarts at zero for every
    /// track, so without this the next sample would look like a jump
    /// backwards and be discarded.
    fn begin(&mut self, now_ms: u64) {
        self.sample = Some((0, now_ms));
    }

    fn observe(&mut self, copied: u64, now_ms: u64) {
        let Some((sampled_bytes, sampled_at)) = self.sample else {
            self.sample = Some((copied, now_ms));
            return;
        };
        let elapsed = now_ms.saturating_sub(sampled_at);
        if elapsed < RATE_SAMPLE_INTERVAL_MS || copied <= sampled_bytes {
            return;
        }
        let gained = copied - sampled_bytes;
        self.bytes_per_second = gained.saturating_mul(1_000) / elapsed;
        self.sample = Some((copied, now_ms));
    }
}

/// One device's run, from the first plan to its outcome.
struct DeviceRun {
    /// Absent while the plan is still being computed — the machine cannot
    /// exist before it knows what it would do.
    machine: Option<DeviceSyncMachine>,
    outcome: Option<SyncOutcome>,
    rate: RateMeter,
    /// Which transfer the rate meter is currently measuring, so entering the
    /// next one resets it.
    measuring: Option<usize>,
}

impl DeviceRun {
    fn planning() -> Self {
        Self {
            machine: None,
            outcome: None,
            rate: RateMeter::default(),
            measuring: None,
        }
    }

    fn is_active(&self) -> bool {
        self.outcome.is_none()
    }

    fn phase(&self) -> &'static str {
        if let Some(outcome) = &self.outcome {
            return match outcome {
                // A finished run leaves the device idle again; only a real
                // failure keeps a state the interface must draw attention to.
                SyncOutcome::Completed { .. } | SyncOutcome::Cancelled => "idle",
                SyncOutcome::Failed { .. } => "failed",
            };
        }
        let Some(machine) = &self.machine else {
            return "inspecting";
        };
        match machine.phase() {
            PlannedSyncPhase::Idle => "idle",
            PlannedSyncPhase::ComputingDelta => "inspecting",
            PlannedSyncPhase::Syncing { step, .. } => match step {
                SyncStep::Removing => "removing",
                SyncStep::Transcoding => "transcoding",
                SyncStep::Copying => "copying",
                SyncStep::WritingPlaylists => "writing_playlists",
            },
            PlannedSyncPhase::Finishing => "verifying",
        }
    }

    fn snapshot(&self, device: &str) -> DeviceRunSnapshot {
        let (current_track, bytes_done, bytes_total) =
            match self.machine.as_ref().map(DeviceSyncMachine::phase) {
                Some(PlannedSyncPhase::Syncing {
                    current_track,
                    bytes_done,
                    bytes_total,
                    ..
                }) => (current_track.clone(), *bytes_done, *bytes_total),
                _ => (String::new(), 0, 0),
            };
        DeviceRunSnapshot {
            device: device.to_owned(),
            phase: self.phase().to_owned(),
            progress: DeviceProgress {
                bytes_done,
                bytes_total,
                bytes_per_second: self.rate.bytes_per_second,
            },
            current_track,
            failed_track_ids: self
                .machine
                .as_ref()
                .map(|machine| machine.failed_tracks().to_vec())
                .unwrap_or_default(),
            outcome: self.outcome.as_ref().map(|outcome| {
                match outcome {
                    SyncOutcome::Completed { .. } => "completed",
                    SyncOutcome::Cancelled => "cancelled",
                    SyncOutcome::Failed { .. } => "failed",
                }
                .to_owned()
            }),
        }
    }
}

/// Every device the runtime has run, or is running, in this process.
pub(crate) struct DeviceRuns {
    runs: BTreeMap<String, DeviceRun>,
}

impl DeviceRuns {
    pub(crate) fn new() -> Self {
        Self {
            runs: BTreeMap::new(),
        }
    }

    /// Whether any run is still going — one of §9.6's four idle conditions.
    pub(crate) fn is_active(&self) -> bool {
        self.runs.values().any(DeviceRun::is_active)
    }

    pub(crate) fn snapshots(&self) -> Vec<DeviceRunSnapshot> {
        self.runs
            .iter()
            .map(|(device, run)| run.snapshot(device))
            .collect()
    }

    pub(crate) fn snapshot(&self, device: &str) -> Option<DeviceRunSnapshot> {
        self.runs.get(device).map(|run| run.snapshot(device))
    }

    /// Begins a run by asking the port what it would change. Two clients
    /// starting the same device is an ordinary race, so the loser is told
    /// the run is already going rather than starting a second one.
    pub(crate) fn start(
        &mut self,
        effects: &dyn DeviceEffects,
        device: &str,
    ) -> Result<(), RuntimeError> {
        if self.runs.get(device).is_some_and(DeviceRun::is_active) {
            return Err(RuntimeError::Rejected(Rejected::DeviceAlreadyRunning));
        }
        self.runs.insert(device.to_owned(), DeviceRun::planning());
        effects.plan(device);
        Ok(())
    }

    /// Answers a [`DeviceEffects::plan`] request. `None` means planning
    /// failed; the run ends there, having touched nothing.
    pub(crate) fn on_plan(
        &mut self,
        effects: &dyn DeviceEffects,
        device: &str,
        plan: Option<MirrorPlan>,
        now_ms: u64,
    ) -> bool {
        let Some(run) = self.runs.get_mut(device) else {
            return false;
        };
        if run.machine.is_some() || !run.is_active() {
            // A late or duplicate answer. Ignoring it is the only safe
            // option: replacing a running machine would strand its effects.
            return false;
        }
        let Some(plan) = plan else {
            run.outcome = Some(SyncOutcome::Failed {
                terminal_error: Some("device_planning".into()),
                failed_tracks: Vec::new(),
            });
            return true;
        };
        run.machine = Some(DeviceSyncMachine::new(device.to_owned(), plan));
        self.dispatch(effects, device, Event::Start, now_ms);
        true
    }

    /// Feeds one effect result back into the machine and forwards whatever
    /// it asks for next.
    pub(crate) fn on_event(
        &mut self,
        effects: &dyn DeviceEffects,
        device: &str,
        event: Event,
        now_ms: u64,
    ) -> bool {
        if !self.runs.contains_key(device) {
            return false;
        }
        self.dispatch(effects, device, event, now_ms)
    }

    pub(crate) fn cancel(
        &mut self,
        effects: &dyn DeviceEffects,
        device: &str,
        now_ms: u64,
    ) -> Result<(), RuntimeError> {
        match self.runs.get(device) {
            Some(run) if run.is_active() => {}
            _ => return Err(RuntimeError::Rejected(Rejected::NoRunToCancel)),
        }
        // Cancelling before the plan arrived: there is no machine to tell,
        // so the run simply ends here. The port's late plan is then ignored
        // by `on_plan`'s inactive-run guard.
        if self.runs[device].machine.is_none() {
            if let Some(run) = self.runs.get_mut(device) {
                run.outcome = Some(SyncOutcome::Cancelled);
            }
            return Ok(());
        }
        self.dispatch(effects, device, Event::Cancel, now_ms);
        Ok(())
    }

    /// The one place a machine is advanced: dispatch, note the outcome,
    /// forward the rest to the port. Returns whether anything observable
    /// changed.
    fn dispatch(
        &mut self,
        effects: &dyn DeviceEffects,
        device: &str,
        event: Event,
        now_ms: u64,
    ) -> bool {
        let Some(run) = self.runs.get_mut(device) else {
            return false;
        };
        let Some(machine) = run.machine.as_mut() else {
            return false;
        };
        if let Event::CopyProgress { copied } = &event {
            run.rate.observe(*copied, now_ms);
        }
        let produced = machine.dispatch(event);
        let mut outstanding = Vec::new();
        for effect in produced {
            match effect {
                Effect::Finished(outcome) => run.outcome = Some(outcome),
                effect => {
                    // Entering a new copy restarts byte progress at zero, so
                    // the meter must start over with it.
                    if let Effect::CopyTrack { index, .. } = &effect {
                        if run.measuring != Some(*index) {
                            run.measuring = Some(*index);
                            run.rate.begin(now_ms);
                        }
                    }
                    outstanding.push(effect);
                }
            }
        }
        for effect in outstanding {
            effects.perform(device, effect);
        }
        true
    }
}

#[cfg(test)]
#[path = "devices_tests.rs"]
mod devices_tests;
