use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use reprise_core::device_sync::podcasts::PodcastSyncPlan;
use reprise_core::device_sync::settings::{
    delete_device_file, delete_device_playlist, upsert_device_file, upsert_device_playlist,
    DeviceFileRecord, DevicePlaylistRecord,
};
use reprise_core::device_sync::{
    load_or_create_targets, DesiredManagedFile, ManagedRemoval, MirrorPlan, StorageId,
    SyncTargetKind, TransferAction,
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

struct PlannedWork {
    device_id: String,
    generation: u64,
    root_uri: String,
    plan: MirrorPlan,
    podcasts: PodcastSyncPlan,
    youtube: PodcastSyncPlan,
    /// Resolved device paths for the three named sync targets (`MTP-38`,
    /// `MTP-23`) — loaded once per sync so every transfer/removal/cleanup
    /// step below routes through the right folder instead of a hard-coded
    /// single managed root.
    playlists_path: String,
    podcasts_path: String,
    youtube_path: String,
    /// The persisted `StorageId` each target was pointed at by the folder
    /// browser (`MTP-31`/`MTP-32`), `None` until repointed. Carried
    /// alongside the paths above so the transfer actually writes to the
    /// storage the user chose instead of `DeviceStorage::storage_root`'s
    /// "prefer internal" guess (finding 1).
    playlists_storage: Option<StorageId>,
    podcasts_storage: Option<StorageId>,
    youtube_storage: Option<StorageId>,
    cancelled: Arc<AtomicBool>,
    cancellable: gio::Cancellable,
}

#[derive(Clone)]
struct TransferOperation {
    desired: DesiredManagedFile,
    previous: Option<DeviceFileRecord>,
}

fn transfer_operations(plan: &MirrorPlan) -> Vec<TransferOperation> {
    let mut operations = plan
        .copy
        .iter()
        .cloned()
        .map(|desired| TransferOperation {
            desired,
            previous: None,
        })
        .chain(
            plan.replace
                .iter()
                .cloned()
                .map(|replacement| TransferOperation {
                    desired: replacement.desired,
                    previous: Some(replacement.existing),
                }),
        )
        .collect::<Vec<_>>();
    operations.sort_by_key(|operation| operation.desired.track.id);
    operations
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

/// The resolved device path for one named sync target (`MTP-38`). Falls
/// back to the kind's design default if somehow absent from the freshly
/// loaded three — `load_or_create_targets` always returns all three, so
/// this is just defense in depth, never the normal path.
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
/// `target_path` counterpart: `None` both when the target has never been
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
            transfer_operations(&device.mirror_plan)
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
            let cancelled = Arc::new(AtomicBool::new(false));
            let cancellable = gio::Cancellable::new();
            device.generation = device.generation.saturating_add(1);
            device.planned_cancel = Some(cancelled.clone());
            device.cancellable = Some(cancellable.clone());
            device.sync_error = None;
            device.mtp_rate.reset();
            device.sync_phase = syncing_phase(
                SyncStep::Removing,
                0,
                device.mirror_plan.remove.len(),
                String::new(),
                0,
                device.mirror_plan.transfer_bytes,
            );
            let targets = load_or_create_targets(&self.conn.borrow(), device_id)
                .map_err(|error| SyncStartError::Planning(error.to_string()))?;
            PlannedWork {
                device_id: device_id.to_string(),
                generation: device.generation,
                root_uri: device.descriptor.root_uri.clone(),
                plan: device.mirror_plan.clone(),
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

async fn run_planned_sync(weak: Weak<DeviceSyncRuntime>, work: PlannedWork) {
    let Some(runtime) = weak.upgrade() else {
        return;
    };
    let mut failures = Vec::new();
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
        }
    }
    let (deferred_replacement_removals, completed_bytes) = if work.cancelled.load(Ordering::SeqCst)
    {
        (Vec::new(), 0)
    } else {
        music_transfer::run_transfers(&runtime, &work, &mut failures).await
    };
    let mut completed_bytes = completed_bytes;
    if !work.cancelled.load(Ordering::SeqCst) && failures.is_empty() {
        let podcast_offset = 0;
        let content_total = work.podcasts.to_copy.len() + work.youtube.to_copy.len();
        completed_bytes = content_transfer::run_content_transfers(
            &runtime,
            &work,
            &work.podcasts_path,
            work.podcasts_storage,
            &work.podcasts.to_copy,
            podcast_offset,
            content_total,
            completed_bytes,
            &mut failures,
        )
        .await;
    }
    if !work.cancelled.load(Ordering::SeqCst) && failures.is_empty() {
        let youtube_offset = work.podcasts.to_copy.len();
        let content_total = work.podcasts.to_copy.len() + work.youtube.to_copy.len();
        content_transfer::run_content_transfers(
            &runtime,
            &work,
            &work.youtube_path,
            work.youtube_storage,
            &work.youtube.to_copy,
            youtube_offset,
            content_total,
            completed_bytes,
            &mut failures,
        )
        .await;
    }
    if !work.cancelled.load(Ordering::SeqCst) && failures.is_empty() {
        run_playlists(&runtime, &work, &mut failures).await;
    }
    if !work.cancelled.load(Ordering::SeqCst) && failures.is_empty() {
        music_transfer::run_removals(
            &runtime,
            &work,
            &deferred_replacement_removals,
            &mut failures,
        )
        .await;
    }
    if !work.cancelled.load(Ordering::SeqCst) && failures.is_empty() {
        let remove_total = work.podcasts.to_remove.len() + work.youtube.to_remove.len();
        content_transfer::run_content_removals(
            &runtime,
            &work,
            &work.podcasts_path,
            work.podcasts_storage,
            &work.podcasts.to_remove,
            0,
            remove_total,
            &mut failures,
        )
        .await;
        content_transfer::run_content_removals(
            &runtime,
            &work,
            &work.youtube_path,
            work.youtube_storage,
            &work.youtube.to_remove,
            work.podcasts.to_remove.len(),
            remove_total,
            &mut failures,
        )
        .await;
    }
    finish_sync(&runtime, &work, failures);
}

async fn run_playlists(
    runtime: &Rc<DeviceSyncRuntime>,
    work: &PlannedWork,
    failures: &mut Vec<i64>,
) {
    let mut successful_sources = HashSet::new();
    let planned_sources = work
        .plan
        .playlist_writes
        .iter()
        .map(|write| write.source.clone())
        .collect::<HashSet<_>>();
    for (index, playlist) in work.plan.playlist_writes.iter().enumerate() {
        if work.cancelled.load(Ordering::SeqCst) {
            break;
        }
        set_phase(
            runtime,
            &work.device_id,
            work.generation,
            syncing_phase(
                SyncStep::WritingPlaylists,
                index,
                work.plan.playlist_writes.len(),
                playlist.source_name.clone(),
                work.plan.transfer_bytes,
                work.plan.transfer_bytes,
            ),
        );
        let name = playlist_stem(&playlist.device_path, &playlist.source_name);
        let write_result = runtime
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
        match write_result {
            Ok(()) => {
                let record = DevicePlaylistRecord {
                    device_serial: work.device_id.clone(),
                    source: playlist.source.clone(),
                    source_name: playlist.source_name.clone(),
                    device_path: playlist.device_path.clone(),
                    last_synced_at: None,
                };
                match upsert_device_playlist(&runtime.conn.borrow(), &record) {
                    Ok(()) => {
                        successful_sources.insert(playlist.source.clone());
                    }
                    Err(error) => {
                        tracing::warn!(playlist = name, %error, "could not update playlist inventory");
                        failures.push(-1);
                    }
                }
            }
            Err(error) => {
                tracing::warn!(playlist = name, %error, "could not write device playlist");
                failures.push(-1);
            }
        }
    }
    if work.cancelled.load(Ordering::SeqCst) || !failures.is_empty() {
        return;
    }
    for playlist in &work.plan.playlist_removals {
        if work.cancelled.load(Ordering::SeqCst) {
            break;
        }
        if planned_sources.contains(&playlist.source)
            && !successful_sources.contains(&playlist.source)
        {
            continue;
        }
        match runtime
            .backend
            .delete_track(
                work.root_uri.clone(),
                work.playlists_path.clone(),
                work.playlists_storage,
                playlist.device_path.clone(),
            )
            .await
        {
            Ok(_) if !planned_sources.contains(&playlist.source) => {
                if let Err(error) = delete_device_playlist(
                    &runtime.conn.borrow(),
                    &work.device_id,
                    &playlist.source,
                ) {
                    tracing::warn!(%error, "could not remove playlist inventory");
                    failures.push(-1);
                }
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, "could not remove managed device playlist");
                failures.push(-1);
            }
        }
    }
}

fn finish_sync(runtime: &Rc<DeviceSyncRuntime>, work: &PlannedWork, mut failures: Vec<i64>) {
    failures.sort_unstable();
    failures.dedup();
    set_phase(
        runtime,
        &work.device_id,
        work.generation,
        PlannedSyncPhase::Finishing,
    );
    let successful = failures.is_empty() && !work.cancelled.load(Ordering::SeqCst);
    {
        let mut devices = runtime.device_states.borrow_mut();
        if let Some(device) = devices
            .iter_mut()
            .find(|device| device.descriptor.id == work.device_id)
        {
            device.cancellable = None;
            device.planned_cancel = None;
            if successful {
                device.resume_planned = false;
                device.sync_error = None;
            }
        }
    }
    if successful {
        runtime.notify();
        runtime.refresh_contents_after_sync(
            &work.device_id,
            work.plan
                .playlist_writes
                .iter()
                .map(|playlist| playlist.source.clone())
                .collect(),
        );
        return;
    }
    let planning_error = runtime.recompute_delta_silent(&work.device_id).err();
    if let Some(device) = runtime
        .device_states
        .borrow_mut()
        .iter_mut()
        .find(|device| device.descriptor.id == work.device_id)
    {
        device.sync_phase = PlannedSyncPhase::Idle;
        device.sync_error = planning_error
            .or_else(|| {
                (!failures.is_empty())
                    .then(|| format!("{} synchronization items failed", failures.len()))
            })
            .map(|message| SyncFailure {
                message,
                failed_tracks: failures,
            });
    }
    runtime.notify();
    runtime.refresh_contents(&work.device_id);
}

fn syncing_phase(
    step: SyncStep,
    done: usize,
    total: usize,
    current_track: String,
    bytes_done: u64,
    bytes_total: u64,
) -> PlannedSyncPhase {
    PlannedSyncPhase::Syncing {
        step,
        done: u32::try_from(done).unwrap_or(u32::MAX),
        total: u32::try_from(total).unwrap_or(u32::MAX),
        current_track,
        bytes_done: bytes_done.min(bytes_total),
        bytes_total,
    }
}

fn track_activity(title: &str, artist: &str) -> String {
    let artist = artist.trim();
    if artist.is_empty() {
        title.to_string()
    } else {
        format!("{title} — {artist}")
    }
}

fn set_phase(
    runtime: &DeviceSyncRuntime,
    device_id: &str,
    generation: u64,
    phase: PlannedSyncPhase,
) {
    if let Some(device) = runtime
        .device_states
        .borrow_mut()
        .iter_mut()
        .find(|device| device.descriptor.id == device_id && device.generation == generation)
    {
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
    runtime.notify();
}

fn update_copy_bytes(
    runtime: &DeviceSyncRuntime,
    device_id: &str,
    generation: u64,
    done: u64,
    total: u64,
    copied: u64,
) {
    if let Some(device) = runtime
        .device_states
        .borrow_mut()
        .iter_mut()
        .find(|device| device.descriptor.id == device_id && device.generation == generation)
    {
        if let PlannedSyncPhase::Syncing {
            bytes_done,
            bytes_total,
            ..
        } = &mut device.sync_phase
        {
            *bytes_done = (*bytes_done).max(done.min(total));
            *bytes_total = total;
            device.mtp_rate.observe(copied, Instant::now());
        }
    }
    runtime.notify();
}

#[path = "device_sync_content_transfer.rs"]
mod content_transfer;
#[path = "device_sync_music_transfer.rs"]
mod music_transfer;

fn temporary_transcode_path(device_id: &str, track_id: i64, extension: &str) -> PathBuf {
    let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let safe_device = reprise_core::device_sync::safe_component(device_id, "device");
    std::env::temp_dir().join(format!(
        "reprise-sync-{safe_device}-{}-{track_id}-{sequence}.{extension}",
        std::process::id(),
    ))
}
