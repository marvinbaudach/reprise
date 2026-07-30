//! Recording what a synchronization run did, while it does it (`MTP-20`).
//!
//! Split out of `device_sync_planned.rs` to keep that module under the
//! 800-line gate. The log must never be able to break a sync, so every write
//! here is best-effort: a failure is logged and dropped rather than
//! propagated, and a run whose opening entry could not be written simply
//! carries no id and records nothing.

use reprise_core::device_sync::sync_log::{self, Deviation, DeviationKind, RunCounters, RunStart};
use reprise_core::device_sync::SyncOutcome;

use super::*;

/// Wall-clock seconds, for log entries a person reads later.
pub(super) fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or_default()
}

/// Records what a run did while it does it (MTP-20).
///
/// The log must never be able to break a sync, so every write is best-effort:
/// a failure is logged and dropped rather than propagated. A run whose opening
/// entry could not be written simply carries no id and records nothing.
pub(super) struct RunLog {
    run: Option<i64>,
    counters: RunCounters,
}

impl RunLog {
    pub(super) fn open(runtime: &DeviceSyncRuntime, start: &RunStart) -> Self {
        let run = match sync_log::start_run(&runtime.conn, start) {
            Ok(run) => Some(run),
            Err(error) => {
                tracing::warn!(%error, "could not open the device sync log entry");
                None
            }
        };
        Self {
            run,
            counters: RunCounters::default(),
        }
    }

    pub(super) fn copied(&mut self, bytes: u64) {
        self.counters.copied = self.counters.copied.saturating_add(1);
        self.counters.bytes_copied = self.counters.bytes_copied.saturating_add(bytes);
    }

    pub(super) fn deleted(&mut self) {
        self.counters.deleted = self.counters.deleted.saturating_add(1);
    }

    pub(super) fn note(
        &mut self,
        runtime: &DeviceSyncRuntime,
        kind: DeviationKind,
        track_id: Option<i64>,
        device_path: &str,
        detail: String,
    ) {
        if matches!(kind, DeviationKind::Failed) {
            self.counters.failed = self.counters.failed.saturating_add(1);
        }
        let Some(run) = self.run else {
            return;
        };
        let deviation = Deviation {
            kind,
            track_id,
            device_path: device_path.to_owned(),
            detail,
        };
        if let Err(error) = sync_log::note_deviation(&runtime.conn, run, &deviation) {
            tracing::warn!(%error, "could not record a device sync deviation");
        }
    }

    pub(super) fn close(
        &self,
        runtime: &DeviceSyncRuntime,
        outcome: &SyncOutcome,
        finished_at: i64,
    ) {
        let Some(run) = self.run else {
            return;
        };
        let summary = sync_log::summarize(outcome, self.counters, finished_at);
        if let Err(error) = sync_log::finish_run(&runtime.conn, run, &summary) {
            tracing::warn!(%error, "could not close the device sync log entry");
        }
    }
}

impl DeviceSyncRuntime {
    /// Reloads this device's recorded runs so the page can show them (MTP-20).
    /// Best-effort: a log that cannot be read leaves the section empty rather
    /// than breaking the page.
    pub(in crate::ui) fn reload_sync_history(&self, device_id: &str) {
        let loaded = {
            let conn = &self.conn;
            match sync_log::recent_runs(conn, sync_log::RETAINED_RUNS) {
                Ok(runs) => runs
                    .into_iter()
                    .filter(|run| run.device_serial == device_id)
                    .map(|run| {
                        let found = sync_log::deviations(conn, run.id).unwrap_or_default();
                        (run, found)
                    })
                    .collect(),
                Err(error) => {
                    tracing::warn!(%error, "could not read the device sync log");
                    Vec::new()
                }
            }
        };
        let mut devices = self.device_states.borrow_mut();
        if let Some(device) = devices
            .iter_mut()
            .find(|device| device.descriptor.id == device_id)
        {
            device.history = loaded;
        }
    }
}
