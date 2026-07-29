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
        let run = match sync_log::start_run(&runtime.conn.borrow(), start) {
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
        if let Err(error) = sync_log::note_deviation(&runtime.conn.borrow(), run, &deviation) {
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
        if let Err(error) = sync_log::finish_run(&runtime.conn.borrow(), run, &summary) {
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
            let conn = self.conn.borrow();
            match sync_log::recent_runs(&conn, sync_log::RETAINED_RUNS) {
                Ok(runs) => runs
                    .into_iter()
                    .filter(|run| run.device_serial == device_id)
                    .map(|run| {
                        let found = sync_log::deviations(&conn, run.id).unwrap_or_default();
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

    pub fn sync_now(self: &Rc<Self>, device_id: &str) -> Result<(), SyncStartError> {
        {
            let devices = self.device_states.borrow();
            let device = devices
                .iter()
                .find(|device| device.descriptor.id == device_id && device.connected)
                .ok_or(SyncStartError::UnknownDevice)?;
            if device.is_busy() {
                return Err(SyncStartError::Busy);
            }
            if device.scanning {
                return Err(SyncStartError::Planning(
                    "device storage inspection is still running".into(),
                ));
            }
            if device.scan_error.is_some() {
                return Err(SyncStartError::Planning(
                    "device storage inspection is unavailable".into(),
                ));
            }
            if device.storage.access == reprise_core::device_sync::DeviceStorageAccess::ReadOnly {
                return Err(SyncStartError::Planning(
                    "device storage is read-only".into(),
                ));
            }
        }
        self.recompute_delta(device_id)
            .map_err(SyncStartError::Planning)?;
        let required_transcode_profiles = {
            let devices = self.device_states.borrow();
            let device = devices
                .iter()
                .find(|device| device.descriptor.id == device_id && device.connected)
                .ok_or(SyncStartError::UnknownDevice)?;
            if !device.mirror_plan.blockers.is_empty() {
                return Err(SyncStartError::Planning(blocker_message(
                    &device.mirror_plan,
                )));
            }
            DeviceSyncMachine::new(device_id.to_string(), device.mirror_plan.clone())
                .transfers()
                .iter()
                .filter_map(|operation| transcode_profile(operation.desired.action))
                .collect::<HashSet<_>>()
        };
        for profile in required_transcode_profiles {
            self.backend
                .probe_transcode(profile)
                .map_err(SyncStartError::Planning)?;
        }
        let work = {
            let mut devices = self.device_states.borrow_mut();
            let device = devices
                .iter_mut()
                .find(|device| device.descriptor.id == device_id && device.connected)
                .ok_or(SyncStartError::UnknownDevice)?;
            if device.is_busy() {
                return Err(SyncStartError::Busy);
            }
            if !device.mirror_plan.blockers.is_empty() {
                return Err(SyncStartError::Planning(blocker_message(
                    &device.mirror_plan,
                )));
            }
            if let Some(available_bytes) = device.storage.free_bytes {
                if device.mirror_plan.transfer_bytes > available_bytes {
                    let error = SyncStartError::InsufficientSpace {
                        required_bytes: device.mirror_plan.transfer_bytes,
                        available_bytes,
                    };
                    device.sync_error = Some(SyncFailure {
                        message: error.to_string(),
                        failed_tracks: Vec::new(),
                    });
                    drop(devices);
                    self.notify();
                    return Err(error);
                }
            }
            let machine = Rc::new(RefCell::new(DeviceSyncMachine::new(
                device_id.to_string(),
                device.mirror_plan.clone(),
            )));
            // The run opens synchronously, so a caller that starts a sync sees
            // the device busy the moment `sync_now` returns rather than one
            // main-loop turn later.
            let pending = machine.borrow_mut().dispatch(Event::Start);
            let cancelled = Arc::new(AtomicBool::new(false));
            let cancellable = gio::Cancellable::new();
            device.sync_phase = machine.borrow().phase().clone();
            device.machine = Some(machine.clone());
            device.planned_cancel = Some(cancelled.clone());
            device.cancellable = Some(cancellable.clone());
            device.sync_error = None;
            device.mtp_rate.reset();
            let targets = load_or_create_targets(&self.conn.borrow(), device_id)
                .map_err(|error| SyncStartError::Planning(error.to_string()))?;
            let log = RunLog::open(
                self,
                &RunStart {
                    device_serial: device_id.to_string(),
                    device_name: device.descriptor.name.clone(),
                    transfer_profile: device.settings.profile.storage_value().to_owned(),
                    started_at: now_seconds(),
                    // The additive content copies count as planned work too
                    // (`MTP-23`); leaving them out would make the log report a
                    // run smaller than the one that actually happened.
                    planned: u32::try_from(
                        device.mirror_plan.copy.len()
                            + device.mirror_plan.replace.len()
                            + device.podcast_plan.to_copy.len()
                            + device.youtube_plan.to_copy.len(),
                    )
                    .unwrap_or(u32::MAX),
                },
            );
            PlannedWork {
                device_id: device_id.to_string(),
                root_uri: device.descriptor.root_uri.clone(),
                machine,
                podcasts: device.podcast_plan.clone(),
                youtube: device.youtube_plan.clone(),
                playlists_path: target_path(&targets, SyncTargetKind::Playlists),
                podcasts_path: target_path(&targets, SyncTargetKind::PodcastEpisodes),
                youtube_path: target_path(&targets, SyncTargetKind::YoutubeAudio),
                playlists_storage: target_storage(&targets, SyncTargetKind::Playlists),
                podcasts_storage: target_storage(&targets, SyncTargetKind::PodcastEpisodes),
                youtube_storage: target_storage(&targets, SyncTargetKind::YoutubeAudio),
                cancelled,
                cancellable,
                transcoded: None,
                pending,
                log,
            }
        };
        self.notify();
        let weak = Rc::downgrade(self);
        gtk4::glib::MainContext::ref_thread_default().spawn_local(async move {
            run_planned_sync(weak, work).await;
        });
        Ok(())
    }

    pub fn eject(self: &Rc<Self>, device_id: &str) {
        let backend = self.backend.clone();
        let id = device_id.to_string();
        gtk4::glib::MainContext::ref_thread_default().spawn_local(async move {
            if let Err(error) = backend.eject(id).await {
                tracing::warn!(%error, "could not eject Android device");
            }
        });
    }
}
