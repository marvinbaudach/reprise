//! Starting a device-sync run and executing the effects its reducer emits.
//!
//! The order of the work — clean partials, transcode, copy, write playlists,
//! remove, verify — is not decided here. It lives in
//! [`reprise_core::device_sync::DeviceSyncMachine`]. This module only starts a
//! run, performs the I/O and database writes the machine asks for, and feeds
//! the outcome back.

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use reprise_core::device_sync::settings::{
    delete_device_file, delete_device_playlist, upsert_device_file, upsert_device_playlist,
    DeviceFileRecord, DevicePlaylistRecord,
};
use reprise_core::device_sync::{
    DeviceSyncMachine, Effect, Event, ManagedRemoval, MirrorPlan, SyncOutcome, TransferAction,
    TransferOperation, TransferSource,
};

use super::*;

static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

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
    machine: Rc<RefCell<DeviceSyncMachine>>,
    /// Interrupts the transcoder, which runs on its own thread.
    cancelled: Arc<AtomicBool>,
    /// Interrupts GIO copies.
    cancellable: gio::Cancellable,
    /// The file the last transcode produced, awaiting its copy.
    transcoded: Option<PathBuf>,
    /// The effects `Event::Start` unlocked, awaiting the first main-loop turn.
    pending: Vec<Effect>,
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

impl DeviceSyncRuntime {
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
            PlannedWork {
                device_id: device_id.to_string(),
                root_uri: device.descriptor.root_uri.clone(),
                machine,
                cancelled,
                cancellable,
                transcoded: None,
                pending,
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
            finish_sync(&runtime, &work, outcome);
            return;
        }
        let event = perform(&runtime, &mut work, effect).await;
        if !is_current_run(&runtime, &work) {
            return;
        }
        work.pending = work.machine.borrow_mut().dispatch(event);
        publish_phase(&runtime, &work);
    }
}

/// Performs one effect and returns the event that answers it.
async fn perform(runtime: &Rc<DeviceSyncRuntime>, work: &mut PlannedWork, effect: Effect) -> Event {
    match effect {
        Effect::Finished(_) => unreachable!("the driver handles Finished before calling perform"),
        Effect::CleanPartials => {
            let result = runtime
                .backend
                .cleanup_partials(work.root_uri.clone())
                .await
                .map(|_| ())
                .map_err(|error| {
                    tracing::warn!(device_id = work.device_id, %error, "could not clean partial sync files");
                    error
                });
            Event::PartialsCleaned(result)
        }
        Effect::Transcode { index, action } => {
            let entry = transfer(work, index).desired.clone();
            let profile =
                transcode_profile(action).expect("a transcode effect must name a transcode action");
            let extension = match profile {
                TranscodeProfile::Opus160 => "opus",
                TranscodeProfile::Mp3(_) => "mp3",
            };
            let request = TranscodeRequest {
                source: entry.track.source_path.clone(),
                output: temporary_transcode_path(&work.device_id, entry.track.id, extension),
                profile,
                metadata: reprise_platform_linux::device_transfer::AudioMetadata::for_track(
                    &entry.track,
                ),
            };
            match runtime
                .backend
                .transcode_track(request, work.cancelled.clone())
                .await
            {
                Ok(file) => {
                    let size = file.size_bytes;
                    work.transcoded = Some(file.path);
                    Event::Transcoded(Ok(size))
                }
                Err(error) => {
                    tracing::warn!(track_id = entry.track.id, %error, "device audio transcode failed");
                    Event::Transcoded(Err(error))
                }
            }
        }
        Effect::CopyTrack {
            index,
            source,
            bytes,
        } => {
            let entry = transfer(work, index).desired.clone();
            let (path, temporary) = match source {
                TransferSource::Original => (entry.track.source_path.clone(), false),
                TransferSource::Transcoded => match work.transcoded.take() {
                    Some(path) => (path, true),
                    None => {
                        return Event::TrackCopied(Err(
                            "the transcoded file went missing before its copy".into(),
                        ))
                    }
                },
            };
            let result = runtime
                .backend
                .replace_track(
                    work.device_id.clone(),
                    work.root_uri.clone(),
                    path.clone(),
                    entry.device_path.clone(),
                    bytes,
                    work.cancellable.clone(),
                    copy_progress(runtime, work),
                )
                .await;
            if temporary {
                let _ = std::fs::remove_file(&path);
            }
            match result {
                Ok(_) => Event::TrackCopied(Ok(bytes)),
                Err(error) => {
                    tracing::warn!(track_id = entry.track.id, %error, "device transfer failed");
                    Event::TrackCopied(Err(error))
                }
            }
        }
        Effect::RecordFile { index, device_size } => {
            let entry = transfer(work, index).desired.clone();
            let record = DeviceFileRecord {
                device_serial: work.device_id.clone(),
                track_id: entry.track.id,
                source_path: entry.track.source_path.to_string_lossy().into_owned(),
                source_size: entry.track.size_bytes,
                source_mtime: entry.track.source_mtime,
                device_path: entry.device_path.clone(),
                device_size,
                profile_fingerprint: entry.profile_fingerprint.clone(),
                pinned: false,
            };
            let result = {
                let conn = runtime.conn.borrow();
                upsert_device_file(&conn, &record)
            };
            Event::FileRecorded(result.map_err(|error| {
                tracing::warn!(track_id = entry.track.id, %error, "could not update device inventory");
                error.to_string()
            }))
        }
        Effect::WritePlaylist { index } => {
            let playlist = playlist_write(work, index);
            let name = playlist_stem(&playlist.device_path, &playlist.source_name);
            let result = runtime
                .backend
                .replace_playlist(
                    work.device_id.clone(),
                    work.root_uri.clone(),
                    name.clone(),
                    playlist.contents.as_bytes().to_vec(),
                )
                .await;
            Event::PlaylistWritten(result.map_err(|error| {
                tracing::warn!(playlist = name, %error, "could not write device playlist");
                error
            }))
        }
        Effect::RecordPlaylist { index } => {
            let playlist = playlist_write(work, index);
            let record = DevicePlaylistRecord {
                device_serial: work.device_id.clone(),
                source: playlist.source.clone(),
                source_name: playlist.source_name.clone(),
                device_path: playlist.device_path.clone(),
                last_synced_at: None,
            };
            let result = upsert_device_playlist(&runtime.conn.borrow(), &record);
            Event::PlaylistRecorded(result.map_err(|error| {
                tracing::warn!(playlist = record.source_name, %error, "could not update playlist inventory");
                error.to_string()
            }))
        }
        Effect::RemovePlaylist { index } => {
            let device_path = playlist_removal(work, index).device_path.clone();
            let result = runtime
                .backend
                .delete_track(work.root_uri.clone(), device_path)
                .await;
            Event::PlaylistRemoved(result.map(|_| ()).map_err(|error| {
                tracing::warn!(%error, "could not remove managed device playlist");
                error
            }))
        }
        Effect::ForgetPlaylist { index } => {
            let source = playlist_removal(work, index).source.clone();
            let result = delete_device_playlist(&runtime.conn.borrow(), &work.device_id, &source);
            Event::PlaylistForgotten(result.map(|_| ()).map_err(|error| {
                tracing::warn!(%error, "could not remove playlist inventory");
                error.to_string()
            }))
        }
        Effect::RemoveTrack { index } => {
            let path = removal_path(&removal(work, index));
            let result = runtime
                .backend
                .delete_track(work.root_uri.clone(), path)
                .await;
            Event::TrackRemoved(result.map(|_| ()).map_err(|error| {
                tracing::warn!(%error, "could not remove managed device item");
                error
            }))
        }
        Effect::ForgetFile { index } => {
            let Some(track_id) = removal_track_id(&removal(work, index)) else {
                return Event::FileForgotten(Ok(()));
            };
            let result = delete_device_file(&runtime.conn.borrow(), &work.device_id, track_id);
            Event::FileForgotten(result.map(|_| ()).map_err(|error| {
                tracing::warn!(track_id, %error, "could not remove device inventory row");
                error.to_string()
            }))
        }
        Effect::RemoveReplacedFile { device_path } => {
            let result = runtime
                .backend
                .delete_track(work.root_uri.clone(), device_path)
                .await;
            Event::ReplacedFileRemoved(result.map(|_| ()).map_err(|error| {
                tracing::warn!(%error, "could not remove replaced device track");
                error
            }))
        }
    }
}

fn transfer(work: &PlannedWork, index: usize) -> TransferOperation {
    work.machine.borrow().transfers()[index].clone()
}

fn playlist_write(work: &PlannedWork, index: usize) -> reprise_core::device_sync::PlaylistWrite {
    work.machine.borrow().plan().playlist_writes[index].clone()
}

fn playlist_removal(work: &PlannedWork, index: usize) -> DevicePlaylistRecord {
    work.machine.borrow().plan().playlist_removals[index].clone()
}

fn removal(work: &PlannedWork, index: usize) -> ManagedRemoval {
    work.machine.borrow().plan().remove[index].clone()
}

/// Feeds byte counts from a copy in flight straight into the machine.
fn copy_progress(runtime: &Rc<DeviceSyncRuntime>, work: &PlannedWork) -> Rc<dyn Fn(u64, u64)> {
    let weak_runtime = Rc::downgrade(runtime);
    let machine = work.machine.clone();
    let device_id = work.device_id.clone();
    Rc::new(move |copied, _| {
        let Some(runtime) = weak_runtime.upgrade() else {
            return;
        };
        machine
            .borrow_mut()
            .dispatch(Event::CopyProgress { copied });
        let phase = machine.borrow().phase().clone();
        if let Some(device) = runtime
            .device_states
            .borrow_mut()
            .iter_mut()
            .find(|device| device.descriptor.id == device_id)
        {
            let is_current = device
                .machine
                .as_ref()
                .is_some_and(|current| Rc::ptr_eq(current, &machine));
            if !is_current {
                return;
            }
            device.sync_phase = phase;
            device.mtp_rate.observe(copied, Instant::now());
        }
        runtime.notify();
    })
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
    if let Some(device) = runtime
        .device_states
        .borrow_mut()
        .iter_mut()
        .find(|device| device.descriptor.id == work.device_id)
    {
        let is_current = device
            .machine
            .as_ref()
            .is_some_and(|current| Rc::ptr_eq(current, &work.machine));
        if !is_current {
            return;
        }
        device.sync_phase = phase;
    }
    runtime.notify();
}

fn finish_sync(runtime: &Rc<DeviceSyncRuntime>, work: &PlannedWork, outcome: SyncOutcome) {
    if !is_current_run(runtime, work) {
        return;
    }
    publish_phase(runtime, work);
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
            if successful {
                device.resume_planned = false;
                device.sync_error = None;
            }
        }
    }
    if let SyncOutcome::Completed { verified_sources } = outcome {
        runtime.notify();
        runtime.refresh_contents_after_sync(&work.device_id, verified_sources);
        return;
    }

    let (message, failed_tracks) = match outcome {
        SyncOutcome::Failed {
            message,
            failed_tracks,
        } => (Some(message), failed_tracks),
        _ => (None, Vec::new()),
    };
    // A run can end on a device whose plan no longer describes reality, so the
    // fresh planning error is the more useful message when there is one.
    let planning_error = runtime.recompute_delta_silent(&work.device_id).err();
    if let Some(device) = runtime
        .device_states
        .borrow_mut()
        .iter_mut()
        .find(|device| device.descriptor.id == work.device_id)
    {
        device.sync_phase = PlannedSyncPhase::Idle;
        device.sync_error = planning_error.or(message).map(|message| SyncFailure {
            message,
            failed_tracks,
        });
    }
    runtime.notify();
    runtime.refresh_contents(&work.device_id);
}

fn temporary_transcode_path(device_id: &str, track_id: i64, extension: &str) -> PathBuf {
    let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let safe_device = reprise_core::device_sync::safe_component(device_id, "device");
    std::env::temp_dir().join(format!(
        "reprise-sync-{safe_device}-{}-{track_id}-{sequence}.{extension}",
        std::process::id(),
    ))
}
