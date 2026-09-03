//! Starting a device-sync run and executing the effects its reducer emits.
//!
//! The order of the work — clean partials, transcode, copy, write playlists,
//! remove, verify — is not decided here. It lives in
//! [`reprise_core::device_sync::DeviceSyncMachine`]. This module only starts a
//! run, performs the I/O and database writes the machine asks for, and feeds
//! the outcome back.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

use reprise_core::device_sync::settings::{
    delete_device_file, delete_device_playlist, upsert_device_file, upsert_device_playlist,
    DeviceFileRecord, DevicePlaylistRecord,
};
use reprise_core::device_sync::sync_log::{DeviationKind, RunStart};
use reprise_core::device_sync::{
    CopiedTrack, DeviceSyncMachine, Effect, Event, ManagedRemoval, MirrorPlan, StorageId,
    SyncOutcome, TransferAction, TransferOperation, TransferSource,
};

use super::*;
pub(in crate::ui::device_sync) use run_log::{now_seconds, RunLog};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui::device_sync) enum SyncInitiator {
    Automatic,
    Listener,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncStartError {
    UnknownDevice,
    Busy,
    InsufficientSpace {
        required_bytes: u64,
        available_bytes: u64,
    },
    Planning(String),
}

impl fmt::Display for SyncStartError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownDevice => formatter.write_str("device is not connected"),
            Self::Busy => formatter.write_str("device synchronization is already active"),
            Self::InsufficientSpace {
                required_bytes,
                available_bytes,
            } => write!(
                formatter,
                "sync needs {required_bytes} bytes but only {available_bytes} bytes are available"
            ),
            Self::Planning(error) => {
                write!(formatter, "could not prepare synchronization: {error}")
            }
        }
    }
}

impl std::error::Error for SyncStartError {}

/// Everything one run needs that is not part of its reducer.
///
/// The machine is shared with the device's state entry, so
/// [`DeviceSyncRuntime::cancel_current`] can reach it and so this run can tell
/// whether it is still the current one — an `Rc::ptr_eq` against the entry
/// replaces the generation counter the previous implementation carried.
struct PlannedWork {
    device_id: String,
    root_uri: String,
    /// Whether the platform supplied a durable identity for database state.
    /// A session-only device still transfers and records its run, but writes
    /// no identity-bound inventory under its volatile URI.
    persist_device_state: bool,
    machine: Rc<RefCell<DeviceSyncMachine>>,
    playlists_path: String,
    playlists_storage: Option<StorageId>,
    /// Interrupts the transcoder, which runs on its own thread.
    cancelled: Arc<AtomicBool>,
    /// Interrupts GIO copies.
    cancellable: gio::Cancellable,
    /// Completed transcodes awaiting their matching indexed copy.
    transcoded: HashMap<usize, PathBuf>,
    /// Encodes already running ahead of the machine's current transfer.
    /// Their cancellation and both staged-output cleanup paths live in
    /// `device_sync_transcode_prefetch.rs` beside this type's `Drop` impl.
    transcode_ahead: HashMap<usize, transcode_prefetch::PendingTranscode>,
    /// The effects `Event::Start` unlocked, awaiting the first main-loop turn.
    pending: Vec<Effect>,
    /// What this run did, recorded as it happens (MTP-20).
    log: RunLog,
}

impl PlannedWork {
    fn transfer(&self, index: usize) -> Option<TransferOperation> {
        self.machine.borrow().transfers().get(index).cloned()
    }
}

fn transcode_profile(action: TransferAction) -> Option<TranscodeProfile> {
    match action {
        TransferAction::CopyOriginal => None,
        TransferAction::TranscodeOpus160 => Some(TranscodeProfile::Opus160),
        TransferAction::TranscodeMp3(quality) => Some(TranscodeProfile::Mp3(quality)),
    }
}

fn blocker_message(plan: &MirrorPlan) -> String {
    format!("playlist mirror is blocked: {:?}", plan.blockers)
}

fn playlist_stem(device_path: &str, fallback: &str) -> String {
    Path::new(device_path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(fallback)
        .to_string()
}

fn removal_path(removal: &ManagedRemoval) -> String {
    match removal {
        ManagedRemoval::Inventory(file) => file.device_path.clone(),
        ManagedRemoval::Orphan(file) => file.relative_path.clone(),
    }
}

fn removal_track_id(removal: &ManagedRemoval) -> Option<i64> {
    match removal {
        ManagedRemoval::Inventory(file) => Some(file.track_id),
        ManagedRemoval::Orphan(_) => None,
    }
}

/// The library file this removal mirrored, when the inventory knows it.
///
/// An orphan is a file on the device that no inventory row accounts for, so
/// there is nothing to trace it back to — and `LYR-7` only ever removes a
/// device-side `.lrc` it can prove Reprise put there (see
/// `effects::remove_lyrics_sidecar`).
fn removal_source_path(removal: &ManagedRemoval) -> Option<PathBuf> {
    match removal {
        ManagedRemoval::Inventory(file) => Some(PathBuf::from(&file.source_path)),
        ManagedRemoval::Orphan(_) => None,
    }
}

pub(in crate::ui::device_sync) fn record_rejected_start(
    runtime: &DeviceSyncRuntime,
    log: &RunLog,
    error: &SyncStartError,
) {
    let outcome = match error {
        SyncStartError::UnknownDevice | SyncStartError::Busy => SyncOutcome::Cancelled,
        SyncStartError::InsufficientSpace { .. } | SyncStartError::Planning(_) => {
            SyncOutcome::Failed {
                terminal_error: Some(error.to_string()),
                failed_tracks: Vec::new(),
                verified_sources: Vec::new(),
            }
        }
    };
    log.close(runtime, &outcome, now_seconds());
    runtime.notify();
}

/// Drives one run to its end.
///
/// The machine emits at most one actionable effect at a time, so this is a
/// plain request/response loop: perform the effect, hand the outcome back, take
/// whatever the machine unlocked next.
async fn run_planned_sync(weak: Weak<DeviceSyncRuntime>, mut work: PlannedWork) {
    let Some(runtime) = weak.upgrade() else {
        return;
    };
    effects::apply_listen_report(&runtime, &mut work).await;
    loop {
        let Some(effect) = work.pending.pop() else {
            return;
        };
        if let Effect::Finished(outcome) = effect {
            finish_sync(&runtime, &work, outcome);
            return;
        }
        transcode_prefetch::fill(&runtime, &mut work, &effect);
        let event = effects::perform(&runtime, &mut work, effect).await;
        if !is_current_run(&runtime, &work) {
            return;
        }
        let before = work.machine.borrow().units_done();
        work.pending = work.machine.borrow_mut().dispatch(event);
        let after = work.machine.borrow().units_done();
        if after > before {
            let now = Instant::now();
            if let Some(device) = runtime
                .device_states
                .borrow_mut()
                .iter_mut()
                .find(|device| device.descriptor.id == work.device_id)
            {
                device.mtp_rate.complete_units(after - before, now);
            }
        }
        publish_phase(&runtime, &work);
    }
}

/// Whether this run still owns its device.
///
/// A run that was superseded — cancelled and restarted, or ended by a
/// disconnect — must not write into the device entry any more. Identity of the
/// machine answers that without a counter.
fn is_current_run(runtime: &Rc<DeviceSyncRuntime>, work: &PlannedWork) -> bool {
    runtime
        .device_states
        .borrow()
        .iter()
        .find(|device| device.descriptor.id == work.device_id)
        .and_then(|device| device.machine.as_ref())
        .is_some_and(|current| Rc::ptr_eq(current, &work.machine))
}

fn publish_phase(runtime: &Rc<DeviceSyncRuntime>, work: &PlannedWork) {
    let phase = work.machine.borrow().phase().clone();
    {
        let mut devices = runtime.device_states.borrow_mut();
        if let Some(device) = devices
            .iter_mut()
            .find(|device| device.descriptor.id == work.device_id)
        {
            let is_current = device
                .machine
                .as_ref()
                .is_some_and(|current| Rc::ptr_eq(current, &work.machine));
            if is_current {
                // Progress arrives per track, counting from zero each time, so
                // the rate meter needs a fresh baseline whenever a new copy
                // starts. Without it every sample below the previous track's
                // final byte count is discarded and the displayed rate freezes.
                if matches!(
                    phase,
                    PlannedSyncPhase::Syncing { step, .. } if step.reports_transfer_rate()
                ) {
                    device.mtp_rate.begin_copy(Instant::now());
                } else {
                    device.mtp_rate.stop_copy();
                }
                device.sync_phase = phase;
            }
        }
    }
    runtime.notify();
}

#[cfg(test)]
#[test]
fn lyrics_writes_keep_the_mtp_rate_baseline_active() {
    assert!(SyncStep::WritingLyrics.reports_transfer_rate());
}

fn finish_sync(runtime: &Rc<DeviceSyncRuntime>, work: &PlannedWork, outcome: SyncOutcome) {
    if !is_current_run(runtime, work) {
        return;
    }
    publish_phase(runtime, work);
    work.log.close(runtime, &outcome, now_seconds());
    let successful = matches!(outcome, SyncOutcome::Completed { .. });
    let verified_sources = match &outcome {
        SyncOutcome::Completed { verified_sources } => Some(verified_sources.clone()),
        SyncOutcome::Failed {
            verified_sources, ..
        } if !verified_sources.is_empty() => Some(verified_sources.clone()),
        SyncOutcome::Cancelled | SyncOutcome::Failed { .. } => None,
    };
    {
        let mut devices = runtime.device_states.borrow_mut();
        if let Some(device) = devices
            .iter_mut()
            .find(|device| device.descriptor.id == work.device_id)
        {
            device.cancellable = None;
            device.planned_cancel = None;
            device.machine = None;
            device.active_initiator = None;
            if successful {
                device.resume_initiator = None;
                device.sync_error = None;
            }
        }
    }
    if successful {
        runtime.notify();
        runtime.refresh_contents_after_sync(&work.device_id, verified_sources.unwrap_or_default());
        cleanup_staging_if_idle(runtime);
        return;
    }

    let (terminal_error, failed_tracks) = match outcome {
        SyncOutcome::Failed {
            terminal_error,
            failed_tracks,
            ..
        } => (terminal_error, failed_tracks),
        _ => (None, Vec::new()),
    };
    // A stage that failed outright already explains the run, so its message
    // wins and the delta is not recomputed. Otherwise a fresh planning error
    // describes the device better than a count of lost tracks does.
    let message = terminal_error
        .or_else(|| runtime.recompute_delta_silent(&work.device_id).err())
        .or_else(|| {
            (!failed_tracks.is_empty())
                .then(|| format!("{} synchronization items failed", failed_tracks.len()))
        });
    let failure = message.map(|message| SyncFailure {
        message,
        failed_tracks,
    });
    if let Some(device) = runtime
        .device_states
        .borrow_mut()
        .iter_mut()
        .find(|device| device.descriptor.id == work.device_id)
    {
        device.sync_phase = PlannedSyncPhase::Idle;
        device.sync_error = failure.clone();
    }
    runtime.notify();
    if let Some(verified_sources) = verified_sources {
        runtime.refresh_contents_after_sync(&work.device_id, verified_sources);
        restore_failed_sync_error_after_refresh(runtime, &work.device_id, failure);
    } else {
        runtime.refresh_contents(&work.device_id);
    }
    cleanup_staging_if_idle(runtime);
}

/// Verification owns the playlist timestamp, but a successful inspection must
/// not turn the failed run into a successful one. Wait until that inspection
/// settles, then restore the exact failure unless a queued resume already
/// started another run for the device.
fn restore_failed_sync_error_after_refresh(
    runtime: &Rc<DeviceSyncRuntime>,
    device_id: &str,
    failure: Option<SyncFailure>,
) {
    let weak = Rc::downgrade(runtime);
    let device_id = device_id.to_owned();
    gtk4::glib::MainContext::ref_thread_default().spawn_local(async move {
        loop {
            let Some(runtime) = weak.upgrade() else {
                return;
            };
            let state = runtime
                .device_states
                .borrow()
                .iter()
                .find(|device| device.descriptor.id == device_id)
                .map(|device| (device.scanning, device.machine.is_some()));
            match state {
                None | Some((_, true)) => return,
                Some((false, false)) => {
                    {
                        let mut devices = runtime.device_states.borrow_mut();
                        if let Some(device) = devices
                            .iter_mut()
                            .find(|device| device.descriptor.id == device_id)
                        {
                            device.sync_error = failure;
                        }
                    }
                    runtime.notify();
                    return;
                }
                Some((true, false)) => {
                    gtk4::glib::timeout_future(Duration::from_millis(1)).await;
                }
            }
        }
    });
}

fn cleanup_staging_if_idle(runtime: &DeviceSyncRuntime) {
    let another_run_is_active = runtime
        .device_states
        .borrow()
        .iter()
        .any(|device| device.machine.is_some());
    if !another_run_is_active {
        reprise_core::device_sync::staging::cleanup_process_files();
    }
}

impl DeviceSyncRuntime {
    /// Starts a playlists synchronization immediately.
    pub fn sync_now(self: &Rc<Self>, device_id: &str) -> Result<(), SyncStartError> {
        self.start_sync(device_id, SyncInitiator::Listener)
    }

    pub(super) fn sync_automatically(
        self: &Rc<Self>,
        device_id: &str,
    ) -> Result<(), SyncStartError> {
        self.start_sync(device_id, SyncInitiator::Automatic)
    }

    pub(super) fn start_sync(
        self: &Rc<Self>,
        device_id: &str,
        initiator: SyncInitiator,
    ) -> Result<(), SyncStartError> {
        let start = {
            let devices = self.device_states.borrow();
            let device = devices
                .iter()
                .find(|device| {
                    device.descriptor.id == device_id
                        && device.connected
                        && device.session_state.opens_session()
                })
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
            RunStart {
                device_serial: device_id.to_string(),
                device_name: device.settings.device_name.clone(),
                transfer_profile: device.settings.profile.storage_value().to_owned(),
                started_at: now_seconds(),
                planned: 0,
            }
        };
        let log = RunLog::open(self, &start);
        self.start_transfer_now(device_id, initiator, log)
    }

    /// Starts the transfer machine after the run log has opened.
    pub(in crate::ui::device_sync) fn start_transfer_now(
        self: &Rc<Self>,
        device_id: &str,
        initiator: SyncInitiator,
        log: RunLog,
    ) -> Result<(), SyncStartError> {
        let rejection_log = log.clone();
        let result = (|| {
            {
                let devices = self.device_states.borrow();
                let device = devices
                    .iter()
                    .find(|device| {
                        device.descriptor.id == device_id
                            && device.connected
                            && device.session_state.opens_session()
                    })
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
                if device.storage.access == reprise_core::device_sync::DeviceStorageAccess::ReadOnly
                {
                    return Err(SyncStartError::Planning(
                        "device storage is read-only".into(),
                    ));
                }
            }
            if let Err(error) = self.recompute_delta(device_id) {
                return Err(SyncStartError::Planning(error));
            }
            let required_transcode_profiles = {
                let devices = self.device_states.borrow();
                let device = devices
                    .iter()
                    .find(|device| {
                        device.descriptor.id == device_id
                            && device.connected
                            && device.session_state.opens_session()
                    })
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
                if let Err(error) = self.backend.probe_transcode(profile) {
                    return Err(SyncStartError::Planning(error));
                }
            }
            let log = log;
            let work = {
                let mut devices = self.device_states.borrow_mut();
                let device = devices
                    .iter_mut()
                    .find(|device| {
                        device.descriptor.id == device_id
                            && device.connected
                            && device.session_state.opens_session()
                    })
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
                let machine =
                    DeviceSyncMachine::new(device_id.to_string(), device.mirror_plan.clone());
                let machine = if initiator == SyncInitiator::Listener {
                    machine.with_track_metadata_list()
                } else {
                    machine
                };
                let machine = Rc::new(RefCell::new(machine));
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
                device.active_initiator = Some(initiator);
                device.sync_error = None;
                device.mtp_rate.reset();
                device.mtp_rate.begin_run(Instant::now());
                let target = device.target.clone();
                let persist_device_state = device.descriptor.persistent_id.is_some();
                let planned = u32::try_from(
                    device.mirror_plan.copy.len()
                        + device.mirror_plan.replace.len()
                        + device.mirror_plan.analysis_writes.len(),
                )
                .unwrap_or(u32::MAX);
                log.set_planned(self, planned);
                PlannedWork {
                    device_id: device_id.to_string(),
                    root_uri: device.descriptor.root_uri.clone(),
                    persist_device_state,
                    machine,
                    playlists_path: target.path,
                    playlists_storage: target.storage_id,
                    cancelled,
                    cancellable,
                    transcoded: HashMap::new(),
                    transcode_ahead: HashMap::new(),
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
        })();
        if let Err(error) = &result {
            record_rejected_start(self, &rejection_log, error);
        }
        result
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

#[path = "device_sync_effects.rs"]
mod effects;
#[path = "device_sync_run_log.rs"]
mod run_log;
#[path = "device_sync_transcode_effect.rs"]
mod transcode_effect;
#[path = "device_sync_transcode_prefetch.rs"]
mod transcode_prefetch;

#[cfg(test)]
pub(crate) struct PrefetchCleanupEvidence {
    pub(crate) cancelled: bool,
    pub(crate) pending_drained: bool,
    pub(crate) existed_until_encoder_stopped: bool,
}

#[cfg(test)]
impl DeviceSyncRuntime {
    pub(crate) fn supersede_current_run_for_test(&self, device_id: &str) {
        if let Some(device) = self
            .device_states
            .borrow_mut()
            .iter_mut()
            .find(|device| device.descriptor.id == device_id)
        {
            device.machine = None;
        }
    }
}

#[cfg(test)]
pub(crate) async fn cancel_prefetch_for_test(staged_path: PathBuf) -> PrefetchCleanupEvidence {
    let cancellation = Arc::new(AtomicBool::new(false));
    let (release, released) = async_channel::bounded(1);
    let observed_path = staged_path.clone();
    let existed_until_encoder_stopped = Rc::new(Cell::new(false));
    let observed = existed_until_encoder_stopped.clone();
    let handle = gtk4::glib::MainContext::ref_thread_default().spawn_local(async move {
        let _ = released.recv().await;
        observed.set(observed_path.exists());
        Ok(TranscodedFile {
            path: observed_path,
            size_bytes: 0,
        })
    });
    let mut pending = HashMap::from([(
        0,
        transcode_prefetch::PendingTranscode {
            handle,
            run_cancellation: cancellation.clone(),
            staged_path: staged_path.clone(),
        },
    )]);
    transcode_prefetch::cancel_all(&mut pending);
    let _ = release.send(()).await;
    for _ in 0..100 {
        if existed_until_encoder_stopped.get() && !staged_path.exists() {
            break;
        }
        gtk4::glib::timeout_future(Duration::from_millis(1)).await;
    }
    PrefetchCleanupEvidence {
        cancelled: cancellation.load(std::sync::atomic::Ordering::SeqCst),
        pending_drained: pending.is_empty(),
        existed_until_encoder_stopped: existed_until_encoder_stopped.get(),
    }
}

#[cfg(test)]
pub(crate) async fn transcode_without_prefetch_for_test(
    backend: &dyn DeviceBackend,
    device_id: &str,
    entry: &reprise_core::device_sync::DesiredManagedFile,
    action: TransferAction,
) -> Result<TranscodedFile, String> {
    transcode_effect::without_prefetch_for_test(backend, device_id, entry, action).await
}
