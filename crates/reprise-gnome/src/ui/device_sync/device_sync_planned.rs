use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use reprise_core::device_sync::m3u::{render_named_playlist, DevicePlaylistEntry};
use reprise_core::device_sync::settings::{
    delete_device_file, upsert_device_file, DeviceFileRecord,
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
            Self::Busy => formatter.write_str("another device synchronization is active"),
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
    settings: DeviceSettings,
    delta: SyncDelta,
    transfers: Vec<TransferPlanEntry>,
    cancelled: Arc<AtomicBool>,
    cancellable: gio::Cancellable,
}

impl DeviceSyncRuntime {
    pub fn sync_now(self: &Rc<Self>, device_id: &str) -> Result<(), SyncStartError> {
        if self.active_device.borrow().is_some() {
            return Err(SyncStartError::Busy);
        }
        self.recompute_delta(device_id)
            .map_err(SyncStartError::Planning)?;
        let requires_mp3_transcode = {
            let devices = self.device_states.borrow();
            let device = devices
                .iter()
                .find(|device| device.descriptor.id == device_id && device.connected)
                .ok_or(SyncStartError::UnknownDevice)?;
            let copy_ids = device
                .delta
                .as_ref()
                .map(|delta| delta.to_copy.iter().copied().collect::<HashSet<_>>())
                .unwrap_or_default();
            device.transfer_plan.iter().any(|entry| {
                copy_ids.contains(&entry.track.id)
                    && matches!(entry.mode, TransferMode::TranscodeMp3 { .. })
            })
        };
        if requires_mp3_transcode {
            self.backend
                .probe_mp3_transcode()
                .map_err(SyncStartError::Planning)?;
        }
        let work = {
            let mut devices = self.device_states.borrow_mut();
            let device = devices
                .iter_mut()
                .find(|device| device.descriptor.id == device_id && device.connected)
                .ok_or(SyncStartError::UnknownDevice)?;
            let delta = device
                .delta
                .clone()
                .ok_or_else(|| SyncStartError::Planning("delta is unavailable".into()))?;
            if let Some(available_bytes) = device.available_bytes {
                if delta.bytes > available_bytes {
                    let error = SyncStartError::InsufficientSpace {
                        required_bytes: delta.bytes,
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
            device.transfer_started_at = None;
            device.bytes_per_second = 0;
            device.sync_phase = syncing_phase(
                SyncStep::Removing,
                0,
                delta.to_remove.len(),
                String::new(),
                0,
                delta.bytes,
            );
            PlannedWork {
                device_id: device_id.to_string(),
                generation: device.generation,
                root_uri: device.descriptor.root_uri.clone(),
                settings: device.settings.clone(),
                delta,
                transfers: device.transfer_plan.clone(),
                cancelled,
                cancellable,
            }
        };
        self.active_device.replace(Some(device_id.to_string()));
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
    if let Err(error) = runtime
        .backend
        .cleanup_partials(work.root_uri.clone())
        .await
    {
        tracing::warn!(device_id = work.device_id, %error, "could not clean partial sync files");
    }
    let files = load_device_files(&runtime.conn.borrow(), &work.device_id).unwrap_or_default();
    let files_by_id = files
        .iter()
        .map(|file| (file.track_id, file.clone()))
        .collect::<HashMap<_, _>>();

    run_removals(&runtime, &work, &files_by_id, &mut failures).await;
    if !work.cancelled.load(Ordering::SeqCst) {
        run_transfers(&runtime, &work, &files_by_id, &mut failures).await;
    }
    if !work.cancelled.load(Ordering::SeqCst) {
        run_playlists(&runtime, &work, &mut failures).await;
    }
    finish_sync(&runtime, &work, failures);
}

async fn run_removals(
    runtime: &Rc<DeviceSyncRuntime>,
    work: &PlannedWork,
    files: &HashMap<i64, DeviceFileRecord>,
    failures: &mut Vec<i64>,
) {
    for (index, track_id) in work.delta.to_remove.iter().copied().enumerate() {
        if work.cancelled.load(Ordering::SeqCst) {
            break;
        }
        let Some(file) = files.get(&track_id) else {
            continue;
        };
        set_phase(
            runtime,
            &work.device_id,
            work.generation,
            syncing_phase(
                SyncStep::Removing,
                index,
                work.delta.to_remove.len(),
                file.device_path.clone(),
                0,
                work.delta.bytes,
            ),
        );
        match runtime
            .backend
            .delete_track(work.root_uri.clone(), file.device_path.clone())
            .await
        {
            Ok(_) => {
                if let Err(error) =
                    delete_device_file(&runtime.conn.borrow(), &work.device_id, track_id)
                {
                    tracing::warn!(track_id, %error, "could not remove device inventory row");
                    failures.push(track_id);
                }
            }
            Err(error) => {
                tracing::warn!(track_id, %error, "could not remove device track");
                failures.push(track_id);
            }
        }
    }
}

async fn run_transfers(
    runtime: &Rc<DeviceSyncRuntime>,
    work: &PlannedWork,
    existing: &HashMap<i64, DeviceFileRecord>,
    failures: &mut Vec<i64>,
) {
    let copy_ids = work.delta.to_copy.iter().copied().collect::<HashSet<_>>();
    let transfers = work
        .transfers
        .iter()
        .filter(|entry| copy_ids.contains(&entry.track.id))
        .cloned()
        .collect::<Vec<_>>();
    let mut completed_bytes = 0_u64;
    for (token, entry) in transfers.iter().enumerate() {
        if work.cancelled.load(Ordering::SeqCst) {
            break;
        }
        let current_track = track_activity(&entry.track.title, &entry.track.artist);
        let prepared = match entry.mode {
            TransferMode::Copy => {
                Some((entry.track.source_path.clone(), entry.expected_bytes, false))
            }
            TransferMode::TranscodeMp3 { quality } => {
                set_phase(
                    runtime,
                    &work.device_id,
                    work.generation,
                    syncing_phase(
                        SyncStep::Transcoding,
                        token,
                        transfers.len(),
                        current_track.clone(),
                        completed_bytes,
                        work.delta.bytes,
                    ),
                );
                let request = Mp3TranscodeRequest {
                    source: entry.track.source_path.clone(),
                    output: temporary_mp3_path(&work.device_id, entry.track.id),
                    quality,
                    metadata: reprise_platform_linux::device_transfer::Mp3Metadata::for_track(
                        &entry.track,
                    ),
                };
                match runtime
                    .backend
                    .transcode_track(request, work.cancelled.clone())
                    .await
                {
                    Ok(file) => Some((file.path, file.size_bytes, true)),
                    Err(error) => {
                        tracing::warn!(track_id = entry.track.id, %error, "device MP3 transcode failed");
                        None
                    }
                }
            }
        };
        let Some((source, actual_size, temporary)) = prepared else {
            failures.push(entry.track.id);
            continue;
        };
        set_phase(
            runtime,
            &work.device_id,
            work.generation,
            syncing_phase(
                SyncStep::Copying,
                token,
                transfers.len(),
                current_track,
                completed_bytes,
                work.delta.bytes,
            ),
        );
        let progress_runtime = Rc::downgrade(runtime);
        let progress_id = work.device_id.clone();
        let progress_generation = work.generation;
        let base = completed_bytes;
        let estimated = entry.expected_bytes;
        let bytes_total = work.delta.bytes;
        let progress: Rc<dyn Fn(u64, u64)> = Rc::new(move |copied, _| {
            if let Some(runtime) = progress_runtime.upgrade() {
                update_copy_bytes(
                    &runtime,
                    &progress_id,
                    progress_generation,
                    base.saturating_add(copied.min(estimated)),
                    bytes_total,
                );
            }
        });
        let result = runtime
            .backend
            .replace_track(
                work.device_id.clone(),
                work.root_uri.clone(),
                source.clone(),
                entry.device_path.clone(),
                actual_size,
                work.cancellable.clone(),
                progress,
            )
            .await;
        if temporary {
            let _ = std::fs::remove_file(&source);
        }
        match result {
            Ok(_) => {
                if let Some(old) = existing.get(&entry.track.id) {
                    if old.device_path != entry.device_path {
                        let _ = runtime
                            .backend
                            .delete_track(work.root_uri.clone(), old.device_path.clone())
                            .await;
                    }
                }
                let record = DeviceFileRecord {
                    device_serial: work.device_id.clone(),
                    track_id: entry.track.id,
                    source_path: entry.track.source_path.to_string_lossy().into_owned(),
                    source_size: entry.track.size_bytes,
                    source_mtime: entry.track.source_mtime,
                    device_path: entry.device_path.clone(),
                    device_size: actual_size,
                    profile_fingerprint: entry.mode.fingerprint(),
                    pinned: false,
                };
                if let Err(error) = upsert_device_file(&runtime.conn.borrow(), &record) {
                    tracing::warn!(track_id = entry.track.id, %error, "could not update device inventory");
                    failures.push(entry.track.id);
                }
            }
            Err(error) => {
                tracing::warn!(track_id = entry.track.id, %error, "device transfer failed");
                if !work.cancelled.load(Ordering::SeqCst) {
                    failures.push(entry.track.id);
                }
            }
        }
        completed_bytes = completed_bytes.saturating_add(entry.expected_bytes);
    }
}

async fn run_playlists(
    runtime: &Rc<DeviceSyncRuntime>,
    work: &PlannedWork,
    failures: &mut Vec<i64>,
) {
    let playlists =
        match playlist_snapshots(&runtime.conn.borrow(), &work.settings, &work.transfers) {
            Ok(playlists) => playlists,
            Err(error) => {
                tracing::warn!(%error, "could not build device playlists");
                failures.push(-1);
                return;
            }
        };
    for (index, (name, contents)) in playlists.iter().enumerate() {
        set_phase(
            runtime,
            &work.device_id,
            work.generation,
            syncing_phase(
                SyncStep::WritingPlaylists,
                index,
                playlists.len(),
                name.clone(),
                work.delta.bytes,
                work.delta.bytes,
            ),
        );
        if let Err(error) = runtime
            .backend
            .replace_playlist(
                work.device_id.clone(),
                work.root_uri.clone(),
                name.clone(),
                contents.clone(),
            )
            .await
        {
            tracing::warn!(playlist = name, %error, "could not write device playlist");
            failures.push(-1);
        }
    }
}

fn playlist_snapshots(
    conn: &Connection,
    settings: &DeviceSettings,
    transfers: &[TransferPlanEntry],
) -> Result<Vec<(String, Vec<u8>)>, rusqlite::Error> {
    let DeviceSelection::Sources(sources) = &settings.selection else {
        return Ok(Vec::new());
    };
    let paths = transfers
        .iter()
        .map(|entry| (entry.track.id, entry))
        .collect::<HashMap<_, _>>();
    let manual_names = reprise_core::library::playlists::list(conn)?
        .into_iter()
        .map(|playlist| (playlist.id, playlist.name))
        .collect::<HashMap<_, _>>();
    let smart_names = reprise_core::library::playlists::list_smart(conn)?
        .into_iter()
        .map(|playlist| (playlist.id, playlist.name))
        .collect::<HashMap<_, _>>();
    let mut snapshots = Vec::new();
    for source in sources {
        let (view_source, name) = match source {
            reprise_core::device_sync::SelectionSource::Playlist(id) => (
                reprise_core::view_source::ViewSource::Playlist(*id),
                manual_names.get(id),
            ),
            reprise_core::device_sync::SelectionSource::Smart(id) => (
                reprise_core::view_source::ViewSource::Smart(*id),
                smart_names.get(id),
            ),
        };
        let Some(name) = name else {
            continue;
        };
        let ids =
            reprise_core::queries::query_track_ids(conn, &view_source, "title", "asc", "", &[])?;
        let entries = ids
            .into_iter()
            .filter_map(|id| paths.get(&id))
            .map(|entry| DevicePlaylistEntry {
                relative_path: entry.device_path.clone(),
                duration_secs: entry.track.duration_ms.max(0) / 1_000,
                display: if entry.track.artist.trim().is_empty() {
                    entry.track.title.clone()
                } else {
                    format!("{} - {}", entry.track.artist, entry.track.title)
                },
            })
            .collect::<Vec<_>>();
        snapshots.push((name.clone(), render_named_playlist(&entries).into_bytes()));
    }
    Ok(snapshots)
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
    if runtime.active_device.borrow().as_deref() == Some(&work.device_id) {
        runtime.active_device.replace(None);
    }
    let resume = {
        let mut devices = runtime.device_states.borrow_mut();
        if let Some(device) = devices
            .iter_mut()
            .find(|device| device.descriptor.id == work.device_id)
        {
            device.cancellable = None;
            device.planned_cancel = None;
            if failures.is_empty() && !work.cancelled.load(Ordering::SeqCst) {
                device.last_sync = Some(chrono::Utc::now());
                device.resume_planned = false;
            }
            let resume = device.resume_planned && device.connected;
            if resume {
                device.resume_planned = false;
            }
            resume
        } else {
            false
        }
    };
    let planning_error = runtime.recompute_delta(&work.device_id).err();
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
    runtime.refresh_contents_after_sync(&work.device_id);
    runtime.release_and_start_next(&work.device_id);
    if resume && runtime.active_device.borrow().is_none() {
        if let Err(error) = runtime.sync_now(&work.device_id) {
            tracing::warn!(device_id = work.device_id, %error, "could not resume synchronization");
        }
    }
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
        ) && device.transfer_started_at.is_none()
        {
            device.transfer_started_at = Some(Instant::now());
            device.bytes_per_second = 0;
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
            if let Some(started_at) = device.transfer_started_at {
                device.bytes_per_second = transfer_rate(*bytes_done, started_at.elapsed());
            }
        }
    }
    runtime.notify();
}

fn temporary_mp3_path(device_id: &str, track_id: i64) -> PathBuf {
    let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let safe_device = reprise_core::device_sync::safe_component(device_id, "device");
    std::env::temp_dir().join(format!(
        "reprise-sync-{safe_device}-{}-{track_id}-{sequence}.mp3",
        std::process::id()
    ))
}
