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

use reprise_core::device_sync::podcasts::PodcastSyncPlan;
use reprise_core::device_sync::settings::{
    delete_device_file, delete_device_playlist, upsert_device_file, upsert_device_playlist,
    DeviceFileRecord, DevicePlaylistRecord,
};
use reprise_core::device_sync::sync_log::DeviationKind;
use reprise_core::device_sync::{
    load_or_create_targets, DeviceSyncMachine, Effect, Event, ManagedRemoval, MirrorPlan,
    StorageId, SyncOutcome, SyncTargetKind, TransferAction, TransferOperation, TransferSource,
};

use super::*;
use run_log::{now_seconds, RunLog};

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
            let outcome = content_transfer::run_content_phase(&runtime, &mut work, outcome).await;
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
            // One cleanup per named target (`MTP-38`): partials can be left
            // behind in any of the three folders, and only the playlists
            // target lives at the old single managed root.
            let mut result = Ok(());
            for (target_path, storage_id) in [
                (&work.playlists_path, work.playlists_storage),
                (&work.podcasts_path, work.podcasts_storage),
                (&work.youtube_path, work.youtube_storage),
            ] {
                if let Err(error) = runtime
                    .backend
                    .cleanup_partials(work.root_uri.clone(), target_path.clone(), storage_id)
                    .await
                {
                    tracing::warn!(device_id = work.device_id, target_path, %error, "could not clean partial sync files");
                    result = Err(error);
                }
            }
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
                    work.log.note(
                        runtime,
                        DeviationKind::Failed,
                        Some(entry.track.id),
                        &entry.device_path,
                        format!("transcode failed: {error}"),
                    );
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
                    work.playlists_path.clone(),
                    work.playlists_storage,
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
                Ok(_) => {
                    work.log.copied(bytes);
                    Event::TrackCopied(Ok(bytes))
                }
                Err(error) => {
                    tracing::warn!(track_id = entry.track.id, %error, "device transfer failed");
                    work.log.note(
                        runtime,
                        DeviationKind::Failed,
                        Some(entry.track.id),
                        &entry.device_path,
                        format!("copy failed: {error}"),
                    );
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
            let playlist_device_path = playlist.device_path.clone();
            let result = runtime
                .backend
                .replace_playlist(
                    work.device_id.clone(),
                    work.root_uri.clone(),
                    work.playlists_path.clone(),
                    work.playlists_storage,
                    name.clone(),
                    playlist.contents.as_bytes().to_vec(),
                )
                .await;
            Event::PlaylistWritten(result.map_err(|error| {
                tracing::warn!(playlist = name, %error, "could not write device playlist");
                work.log.note(
                    runtime,
                    DeviationKind::PlaylistWriteFailed,
                    None,
                    &playlist_device_path,
                    format!("playlist write failed: {error}"),
                );
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
                .delete_track(
                    work.root_uri.clone(),
                    work.playlists_path.clone(),
                    work.playlists_storage,
                    device_path,
                )
                .await;
            if result.is_ok() {
                work.log.deleted();
            }
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
            let managed = removal(work, index);
            let path = removal_path(&managed);
            let track_id = removal_track_id(&managed);
            let result = runtime
                .backend
                .delete_track(
                    work.root_uri.clone(),
                    work.playlists_path.clone(),
                    work.playlists_storage,
                    path.clone(),
                )
                .await;
            if result.is_ok() {
                work.log.deleted();
                // Deletions are recorded individually: the mirror owns
                // Music/Reprise, so "what did it remove" is exactly the
                // question someone asks afterwards.
                work.log.note(
                    runtime,
                    DeviationKind::Deleted,
                    track_id,
                    &path,
                    "no longer covered by the selection".to_owned(),
                );
            }
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
                .delete_track(
                    work.root_uri.clone(),
                    work.playlists_path.clone(),
                    work.playlists_storage,
                    device_path,
                )
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

fn temporary_transcode_path(device_id: &str, track_id: i64, extension: &str) -> PathBuf {
    let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let safe_device = reprise_core::device_sync::safe_component(device_id, "device");
    std::env::temp_dir().join(format!(
        "reprise-sync-{safe_device}-{}-{track_id}-{sequence}.{extension}",
        std::process::id(),
    ))
}

#[path = "device_sync_content_transfer.rs"]
mod content_transfer;

#[path = "device_sync_run_log.rs"]
mod run_log;
