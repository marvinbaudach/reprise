//! Starting a device-sync run and executing the effects its reducer emits.
//!
//! The order of the work — clean partials, transcode, copy, write playlists,
//! remove, verify — is not decided here. It lives in
//! [`reprise_core::device_sync::DeviceSyncMachine`]. This module only starts a
//! run, performs the I/O and database writes the machine asks for, and feeds
//! the outcome back.

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Instant;

use reprise_core::device_sync::podcasts::PodcastSyncPlan;
use reprise_core::device_sync::settings::{
    delete_device_file, delete_device_playlist, upsert_device_file, upsert_device_playlist,
    DeviceFileRecord, DevicePlaylistRecord,
};
use reprise_core::device_sync::sync_log::{DeviationKind, RunStart};
use reprise_core::device_sync::{
    DeviceSyncMachine, Effect, Event, ManagedRemoval, MirrorPlan, StorageId, SyncOutcome,
    SyncTargetKind, TransferAction, TransferOperation, TransferSource,
};

use super::*;
pub(in crate::ui::device_sync) use run_log::{now_seconds, RunLog};

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
    initiator: SyncInitiator,
    machine: Rc<RefCell<DeviceSyncMachine>>,
    /// The additive content plans (`MTP-23`). The machine above owns only the
    /// music and playlist mirror; podcast episodes and YouTube audio are
    /// diffed against their own candidate lists and run after it, because
    /// they are not authoritative over their folders the way `MTP-17` is.
    podcasts: PodcastSyncPlan,
    youtube: PodcastSyncPlan,
    /// Resolved device paths for the three named sync targets (`MTP-38`,
    /// `MTP-23`) — loaded once per sync so every transfer, removal and
    /// cleanup step routes through the right folder instead of a hard-coded
    /// single managed root.
    playlists_path: String,
    podcasts_path: String,
    youtube_path: String,
    /// The persisted `StorageId` each target was pointed at by the folder
    /// browser (`MTP-31`/`MTP-32`), `None` until repointed. Carried alongside
    /// the paths above so a transfer actually writes to the storage the user
    /// chose rather than `DeviceStorage::storage_root`'s "prefer internal"
    /// guess.
    playlists_storage: Option<StorageId>,
    podcasts_storage: Option<StorageId>,
    youtube_storage: Option<StorageId>,
    /// Interrupts the transcoder, which runs on its own thread.
    cancelled: Arc<AtomicBool>,
    /// Interrupts GIO copies.
    cancellable: gio::Cancellable,
    /// The file the last transcode produced, awaiting its copy. The machine
    /// always follows a successful transcode with its copy, and that copy
    /// deletes the file whether it succeeded or not, so no other path has to.
    transcoded: Option<PathBuf>,
    /// The effects `Event::Start` unlocked, awaiting the first main-loop turn.
    pending: Vec<Effect>,
    /// What this run did, recorded as it happens (MTP-20).
    log: RunLog,
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

/// The resolved device path for one named sync target (`MTP-38`). Falls back
/// to the kind's design default if somehow absent from the freshly loaded
/// three — `load_or_create_targets` always returns all three, so this is
/// defense in depth, never the normal path.
fn target_path(
    targets: &[reprise_core::device_sync::SyncTarget; 3],
    kind: SyncTargetKind,
) -> String {
    targets
        .iter()
        .find(|target| target.kind == kind)
        .map_or_else(
            || kind.default_path().to_string(),
            |target| target.path.clone(),
        )
}

/// The resolved `StorageId` for one named sync target (`MTP-38`), the
/// [`target_path`] counterpart: `None` both when the target has never been
/// repointed by the folder browser and, defensively, when it is somehow
/// absent from the freshly loaded three.
fn target_storage(
    targets: &[reprise_core::device_sync::SyncTarget; 3],
    kind: SyncTargetKind,
) -> Option<StorageId> {
    targets
        .iter()
        .find(|target| target.kind == kind)
        .and_then(|target| target.storage_id)
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
    device_id: &str,
    log: &RunLog,
    error: &SyncStartError,
) {
    let outcome = match error {
        SyncStartError::UnknownDevice | SyncStartError::Busy => SyncOutcome::Cancelled,
        SyncStartError::InsufficientSpace { .. } | SyncStartError::Planning(_) => {
            SyncOutcome::Failed {
                terminal_error: Some(error.to_string()),
                failed_tracks: Vec::new(),
            }
        }
    };
    log.close(runtime, &outcome, now_seconds());
    runtime.reload_sync_history(device_id);
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
    loop {
        let Some(effect) = work.pending.pop() else {
            return;
        };
        if let Effect::Finished(outcome) = effect {
            let outcome = run_analysis_phase(&runtime, &mut work, outcome).await;
            let mut outcome =
                content_transfer::run_content_phase(&runtime, &mut work, outcome).await;
            if matches!(outcome, SyncOutcome::Completed { .. })
                && work.initiator == SyncInitiator::Listener
            {
                if let Err(error) = effects::write_track_metadata_list(&runtime, &work).await {
                    outcome = SyncOutcome::Failed {
                        terminal_error: Some(error),
                        failed_tracks: Vec::new(),
                    };
                }
            }
            finish_sync(&runtime, &work, outcome);
            return;
        }
        let event = effects::perform(&runtime, &mut work, effect).await;
        if !is_current_run(&runtime, &work) {
            return;
        }
        work.pending = work.machine.borrow_mut().dispatch(event);
        publish_phase(&runtime, &work);
    }
}

async fn run_analysis_phase(
    runtime: &Rc<DeviceSyncRuntime>,
    work: &mut PlannedWork,
    outcome: SyncOutcome,
) -> SyncOutcome {
    if !matches!(outcome, SyncOutcome::Completed { .. }) {
        return outcome;
    }
    let writes = work.machine.borrow().plan().analysis_writes.clone();
    let analysis_bytes = writes
        .iter()
        .map(|write| write.size_bytes)
        .fold(0_u64, u64::saturating_add);
    let mut bytes_done = work
        .machine
        .borrow()
        .plan()
        .transfer_bytes
        .saturating_sub(analysis_bytes);
    for (index, write) in writes.iter().enumerate() {
        if !is_current_run(runtime, work)
            || work.machine.borrow().is_cancelled()
            || work.cancelled.load(Ordering::SeqCst)
            || work.cancellable.is_cancelled()
        {
            return SyncOutcome::Cancelled;
        }
        content_transfer::set_content_phase(
            runtime,
            work,
            content_transfer::syncing_phase(
                SyncStep::Copying,
                index,
                writes.len(),
                write.device_path.clone(),
                bytes_done,
                work.machine.borrow().plan().transfer_bytes,
            ),
        );
        if effects::copy_analysis_sidecar(runtime, work, write).await {
            work.log.copied(write.size_bytes);
        }
        bytes_done = bytes_done.saturating_add(write.size_bytes);
    }
    outcome
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PhaseKind {
    Idle,
    ComputingDelta,
    Syncing(SyncStep),
    Finishing,
}

fn phase_kind(phase: &PlannedSyncPhase) -> PhaseKind {
    match phase {
        PlannedSyncPhase::Idle => PhaseKind::Idle,
        PlannedSyncPhase::ComputingDelta => PhaseKind::ComputingDelta,
        PlannedSyncPhase::Syncing { step, .. } => PhaseKind::Syncing(*step),
        PlannedSyncPhase::Finishing => PhaseKind::Finishing,
    }
}

fn publish_phase(runtime: &Rc<DeviceSyncRuntime>, work: &PlannedWork) {
    let phase = work.machine.borrow().phase().clone();
    let mut changed_kind = false;
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
                changed_kind = phase_kind(&device.sync_phase) != phase_kind(&phase);
                // Progress arrives per track, counting from zero each time, so
                // the rate meter needs a fresh baseline whenever a new copy
                // starts. Without it every sample below the previous track's
                // final byte count is discarded and the displayed rate freezes.
                if matches!(
                    phase,
                    PlannedSyncPhase::Syncing {
                        step: SyncStep::Copying,
                        ..
                    }
                ) {
                    device.mtp_rate.begin_copy(Instant::now());
                }
                device.sync_phase = phase;
            }
        }
    }
    if changed_kind {
        runtime.reload_sync_history(&work.device_id);
    }
    runtime.notify();
}

fn finish_sync(runtime: &Rc<DeviceSyncRuntime>, work: &PlannedWork, outcome: SyncOutcome) {
    if !is_current_run(runtime, work) {
        return;
    }
    publish_phase(runtime, work);
    work.log.close(runtime, &outcome, now_seconds());
    runtime.reload_sync_history(&work.device_id);
    let successful = matches!(outcome, SyncOutcome::Completed { .. });
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
    if let SyncOutcome::Completed { verified_sources } = outcome {
        runtime.notify();
        runtime.refresh_contents_after_sync(&work.device_id, verified_sources);
        return;
    }

    let (terminal_error, failed_tracks) = match outcome {
        SyncOutcome::Failed {
            terminal_error,
            failed_tracks,
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
    if let Some(device) = runtime
        .device_states
        .borrow_mut()
        .iter_mut()
        .find(|device| device.descriptor.id == work.device_id)
    {
        device.sync_phase = PlannedSyncPhase::Idle;
        device.sync_error = message.map(|message| SyncFailure {
            message,
            failed_tracks,
        });
    }
    runtime.notify();
    runtime.refresh_contents(&work.device_id);
}

impl DeviceSyncRuntime {
    /// `MTP-43`: the entry point every "Sync now"/"Download & sync" click
    /// goes through. `MTP-42`'s `primary_action` — read from the phase
    /// `recompute_delta_silent` already keeps current, never re-derived here
    /// — decides whether this run starts with a preparation download
    /// (`preparation::begin_prepared_sync`) or goes straight to the
    /// transfer machine ([`Self::start_transfer_now`]). The precondition
    /// checks below apply to both paths and run synchronously so a busy or
    /// disconnected device is rejected immediately, exactly as before this
    /// split existed.
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
        let (prepare, start) = {
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
            let prepare = preparation::should_prepare(
                reprise_core::device_sync::primary_action(&device.preparation),
                &device.preparation_missing,
            )
            .then(|| device.preparation_missing.clone());
            let start = RunStart {
                device_serial: device_id.to_string(),
                device_name: device.settings.device_name.clone(),
                transfer_profile: device.settings.profile.storage_value().to_owned(),
                started_at: now_seconds(),
                planned: 0,
            };
            (prepare, start)
        };
        let log = RunLog::open(self, &start);
        self.reload_sync_history(device_id);
        if let Some(missing) = prepare {
            preparation::begin_prepared_sync(self, device_id, missing, initiator, log);
            return Ok(());
        }
        self.start_transfer_now(device_id, initiator, log)
    }

    /// The transfer-machine half of a run — unchanged from before `MTP-43`
    /// except for its name and visibility, which widened from `pub` to
    /// `pub(super)` so [`preparation::begin_prepared_sync`]'s async
    /// continuation can call it directly once every preparation download has
    /// been attempted.
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
                device.active_initiator = Some(initiator);
                device.sync_error = None;
                device.mtp_rate.reset();
                let targets = device.targets.clone();
                let persist_device_state = device.descriptor.persistent_id.is_some();
                let planned = u32::try_from(
                    device.mirror_plan.copy.len()
                        + device.mirror_plan.replace.len()
                        + device.mirror_plan.analysis_writes.len()
                        + device.podcast_plan.to_copy.len()
                        + device.youtube_plan.to_copy.len(),
                )
                .unwrap_or(u32::MAX);
                log.set_planned(self, planned);
                PlannedWork {
                    device_id: device_id.to_string(),
                    root_uri: device.descriptor.root_uri.clone(),
                    persist_device_state,
                    initiator,
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
        })();
        if let Err(error) = &result {
            record_rejected_start(self, device_id, &rejection_log, error);
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

#[path = "device_sync_content_transfer.rs"]
mod content_transfer;
#[path = "device_sync_effects.rs"]
mod effects;
#[path = "device_sync_run_log.rs"]
mod run_log;

#[cfg(test)]
mod phase_kind_tests {
    use super::*;

    fn copying(done: u32) -> PlannedSyncPhase {
        PlannedSyncPhase::Syncing {
            step: SyncStep::Copying,
            done,
            total: 10,
            current_track: "Track".into(),
            bytes_done: u64::from(done),
            bytes_total: 10,
        }
    }

    #[test]
    fn history_reload_key_ignores_progress_ticks_but_changes_with_the_step() {
        assert_eq!(phase_kind(&copying(1)), phase_kind(&copying(9)));
        assert_ne!(
            phase_kind(&copying(9)),
            phase_kind(&PlannedSyncPhase::Syncing {
                step: SyncStep::WritingPlaylists,
                done: 0,
                total: 1,
                current_track: String::new(),
                bytes_done: 0,
                bytes_total: 0,
            })
        );
    }
}
