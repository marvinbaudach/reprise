//! Performs one `Effect` the core sync machine emits and answers with the
//! matching `Event` — split out of `device_sync_planned.rs` to keep that file
//! under the project's 800-line limit. Nothing here decides *what* to do next;
//! that stays `DeviceSyncMachine`'s job (see the module doc on `device_sync_planned.rs`).

use super::*;

/// Best-effort phone-to-desktop input at the start of every transfer run.
/// Applying the database transaction before publishing its acknowledgement
/// is the ordering invariant that prevents a returned listen from being lost.
pub(super) async fn apply_listen_report(runtime: &Rc<DeviceSyncRuntime>, work: &mut PlannedWork) {
    use reprise_core::device_sync::listen_report::{
        apply_listen_report, ListenReport, ListenReportAcknowledgement, ACKNOWLEDGEMENT_FILE_NAME,
        REPORT_FILE_NAME,
    };

    let bytes = match runtime
        .backend
        .read_managed_file(
            work.root_uri.clone(),
            work.playlists_path.clone(),
            work.playlists_storage,
            REPORT_FILE_NAME.into(),
        )
        .await
    {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return,
        Err(error) => {
            tracing::warn!(device_id = work.device_id, %error, "could not read phone listen report");
            return;
        }
    };
    if !is_current_run(runtime, work) {
        return;
    }
    if !work.persist_device_state {
        tracing::warn!(
            device_id = work.device_id,
            "ignored phone listen report without a durable device identity"
        );
        return;
    }
    let report = match ListenReport::decode(&bytes) {
        Ok(report) => report,
        Err(error) => {
            tracing::warn!(device_id = work.device_id, %error, "ignored malformed phone listen report");
            return;
        }
    };
    let summary = match apply_listen_report(&runtime.conn, &work.device_id, &report) {
        Ok(summary) => summary,
        Err(error) => {
            tracing::warn!(device_id = work.device_id, %error, "could not apply phone listen report");
            return;
        }
    };
    work.log.returned_report(runtime, &summary);
    let Some(sequence) = summary.acknowledged_sequence else {
        return;
    };
    let acknowledgement = ListenReportAcknowledgement::new(sequence).encode();
    let temporary_path = match reprise_core::device_sync::staging::stage_bytes(
        &work.device_id,
        0,
        "listen-report-ack",
        &acknowledgement,
    ) {
        Ok(path) => path,
        Err(error) => {
            tracing::warn!(device_id = work.device_id, %error, "could not stage phone listen acknowledgement");
            return;
        }
    };
    let result = runtime
        .backend
        .replace_track(
            work.device_id.clone(),
            work.root_uri.clone(),
            work.playlists_path.clone(),
            work.playlists_storage,
            temporary_path.clone(),
            ACKNOWLEDGEMENT_FILE_NAME.into(),
            acknowledgement.len() as u64,
            work.cancellable.clone(),
            Rc::new(|_, _| {}),
        )
        .await;
    reprise_core::device_sync::staging::discard(&temporary_path);
    if let Err(error) = result {
        tracing::warn!(device_id = work.device_id, %error, "could not write phone listen acknowledgement");
    }
}

/// Performs one effect and returns the event that answers it.
pub(super) async fn perform(
    runtime: &Rc<DeviceSyncRuntime>,
    work: &mut PlannedWork,
    effect: Effect,
) -> Event {
    match effect {
        Effect::Finished(_) => unreachable!("the driver handles Finished before calling perform"),
        Effect::CleanPartials => {
            let result = runtime
                .backend
                .cleanup_partials(
                    work.root_uri.clone(),
                    work.playlists_path.clone(),
                    work.playlists_storage,
                )
                .await
                .map(|_| ());
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
                output: reprise_core::device_sync::staging::temporary_path(
                    &work.device_id,
                    entry.track.id,
                    extension,
                ),
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
                reprise_core::device_sync::staging::discard(&path);
            }
            match result {
                Ok(_) => {
                    copy_lyrics_sidecar(
                        runtime,
                        work,
                        &entry.track.source_path,
                        &entry.device_path,
                        &work.playlists_path,
                        work.playlists_storage,
                    )
                    .await;
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
            if !work.persist_device_state {
                return Event::FileRecorded(Ok(()));
            }
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
                let conn = &runtime.conn;
                upsert_device_file(conn, &record)
            };
            Event::FileRecorded(result.map_err(|error| {
                tracing::warn!(track_id = entry.track.id, %error, "could not update device inventory");
                error.to_string()
            }))
        }
        Effect::WriteAnalysis { index } => {
            let planned = work.machine.borrow().plan().analysis_writes[index].clone();
            Event::AnalysisWritten(copy_analysis_sidecar(runtime, work, &planned).await)
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
            if !work.persist_device_state {
                return Event::PlaylistRecorded(Ok(()));
            }
            let playlist = playlist_write(work, index);
            let record = DevicePlaylistRecord {
                device_serial: work.device_id.clone(),
                source: playlist.source.clone(),
                source_name: playlist.source_name.clone(),
                device_path: playlist.device_path.clone(),
                last_synced_at: Some(now_seconds()),
            };
            let result = upsert_device_playlist(&runtime.conn, &record);
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
            if !work.persist_device_state {
                return Event::PlaylistForgotten(Ok(()));
            }
            let source = playlist_removal(work, index).source.clone();
            let result = delete_device_playlist(&runtime.conn, &work.device_id, &source);
            Event::PlaylistForgotten(result.map(|_| ()).map_err(|error| {
                tracing::warn!(%error, "could not remove playlist inventory");
                error.to_string()
            }))
        }
        Effect::RemoveTrack { index } => {
            let managed = removal(work, index);
            let path = removal_path(&managed);
            let track_id = removal_track_id(&managed);
            let source_path = removal_source_path(&managed);
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
                remove_lyrics_sidecar(
                    runtime,
                    work,
                    source_path.as_deref(),
                    &path,
                    &work.playlists_path,
                    work.playlists_storage,
                )
                .await;
                remove_analysis_sidecar(
                    runtime,
                    work,
                    &path,
                    &work.playlists_path,
                    work.playlists_storage,
                )
                .await;
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
            if !work.persist_device_state {
                return Event::FileForgotten(Ok(()));
            }
            let Some(track_id) = removal_track_id(&removal(work, index)) else {
                return Event::FileForgotten(Ok(()));
            };
            let result = delete_device_file(&runtime.conn, &work.device_id, track_id);
            Event::FileForgotten(result.map(|_| ()).map_err(|error| {
                tracing::warn!(track_id, %error, "could not remove device inventory row");
                error.to_string()
            }))
        }
        Effect::RemoveReplacedFile { device_path } => {
            let lyrics_are_still_current = replacement_keeps_lyrics_sidecar(work, &device_path);
            let source_path = replaced_source_path(work, &device_path);
            let result = runtime
                .backend
                .delete_track(
                    work.root_uri.clone(),
                    work.playlists_path.clone(),
                    work.playlists_storage,
                    device_path.clone(),
                )
                .await;
            if result.is_ok() {
                if !lyrics_are_still_current {
                    remove_lyrics_sidecar(
                        runtime,
                        work,
                        source_path.as_deref(),
                        &device_path,
                        &work.playlists_path,
                        work.playlists_storage,
                    )
                    .await;
                }
                // Analysis is derived from the exact audio bytes. Never retain
                // an old sidecar across an audio replacement, even when the
                // projected device filename remains the same.
                remove_analysis_sidecar(
                    runtime,
                    work,
                    &device_path,
                    &work.playlists_path,
                    work.playlists_storage,
                )
                .await;
            }
            Event::ReplacedFileRemoved(result.map(|_| ()).map_err(|error| {
                tracing::warn!(%error, "could not remove replaced device track");
                error
            }))
        }
        Effect::WriteTrackMetadataList => {
            Event::TrackMetadataListWritten(write_track_metadata_list(runtime, work).await)
        }
    }
}

pub(super) async fn copy_analysis_sidecar(
    runtime: &Rc<DeviceSyncRuntime>,
    work: &mut PlannedWork,
    planned: &reprise_core::device_sync::AnalysisSidecarWrite,
) -> Result<u64, String> {
    let track_id = planned.track_id;
    let sidecar = match reprise_core::device_sync::analysis_sidecar::AnalysisSidecar::for_track(
        &runtime.conn,
        track_id,
    ) {
        Ok(Some(sidecar)) => sidecar,
        Ok(None) => return Err("analysis data disappeared before it could be written".into()),
        Err(error) => {
            tracing::warn!(track_id, %error, "could not load analysis sidecar data");
            return Err(format!("could not load analysis sidecar data: {error}"));
        }
    };
    let bytes = match sidecar.encode() {
        Ok(bytes) => bytes,
        Err(error) => {
            tracing::warn!(track_id, %error, "could not encode analysis sidecar data");
            return Err(format!("could not encode analysis sidecar data: {error}"));
        }
    };
    let temporary_path = match reprise_core::device_sync::staging::stage_bytes(
        &work.device_id,
        track_id,
        "analysis",
        &bytes,
    ) {
        Ok(path) => path,
        Err(error) => {
            tracing::warn!(track_id, %error, "could not stage analysis sidecar data");
            return Err(error.to_string());
        }
    };
    let result = runtime
        .backend
        .replace_track(
            work.device_id.clone(),
            work.root_uri.clone(),
            work.playlists_path.clone(),
            work.playlists_storage,
            temporary_path.clone(),
            planned.device_path.clone(),
            bytes.len() as u64,
            work.cancellable.clone(),
            copy_progress(runtime, work),
        )
        .await;
    reprise_core::device_sync::staging::discard(&temporary_path);
    if let Err(error) = result {
        tracing::warn!(track_id, device_path = planned.device_path, %error, "could not copy analysis sidecar to device");
        return Err(format!("could not copy analysis sidecar: {error}"));
    }
    work.log.copied(bytes.len() as u64);
    Ok(bytes.len() as u64)
}

pub(super) async fn write_track_metadata_list(
    runtime: &Rc<DeviceSyncRuntime>,
    work: &PlannedWork,
) -> Result<(), String> {
    let desired_files = work.machine.borrow().plan().desired_files.clone();
    let mut entries = Vec::with_capacity(desired_files.len());
    for desired in desired_files {
        let Some(track) =
            reprise_core::queries::query_present_track_by_id(&runtime.conn, desired.track.id)
                .map_err(|error| format!("could not read track metadata: {error}"))?
        else {
            continue;
        };
        entries.push(
            reprise_core::device_sync::track_metadata_list::TrackMetadataEntry {
                device_path: desired.device_path,
                rating: track.rating,
                play_count: track.play_count,
            },
        );
    }
    entries.sort_by(|left, right| left.device_path.cmp(&right.device_path));
    let bytes = reprise_core::device_sync::track_metadata_list::TrackMetadataList::new(entries)
        .encode()
        .map_err(|error| format!("could not encode track metadata list: {error}"))?;
    let temporary_path = reprise_core::device_sync::staging::stage_bytes(
        &work.device_id,
        0,
        "track-metadata",
        &bytes,
    )
    .map_err(|error| error.to_string())?;
    let result = runtime
        .backend
        .replace_track(
            work.device_id.clone(),
            work.root_uri.clone(),
            work.playlists_path.clone(),
            work.playlists_storage,
            temporary_path.clone(),
            reprise_core::device_sync::track_metadata_list::FILE_NAME.into(),
            bytes.len() as u64,
            work.cancellable.clone(),
            Rc::new(|_, _| {}),
        )
        .await;
    reprise_core::device_sync::staging::discard(&temporary_path);
    result
        .map(|_| ())
        .map_err(|error| format!("could not write track metadata list: {error}"))
}

pub(super) async fn copy_lyrics_sidecar(
    runtime: &Rc<DeviceSyncRuntime>,
    work: &PlannedWork,
    source_path: &Path,
    device_path: &str,
    target_path: &str,
    storage_id: Option<StorageId>,
) {
    let Some(sidecar) =
        reprise_core::device_sync::lyrics_sidecar::paths_for_track(source_path, device_path)
    else {
        return;
    };
    let Some(source_bytes) =
        reprise_core::device_sync::lyrics_sidecar::source_file_size(&sidecar.source_path)
    else {
        return;
    };
    let result = runtime
        .backend
        .replace_track(
            work.device_id.clone(),
            work.root_uri.clone(),
            target_path.to_string(),
            storage_id,
            sidecar.source_path.clone(),
            sidecar.device_path.clone(),
            source_bytes,
            work.cancellable.clone(),
            Rc::new(|_, _| {}),
        )
        .await;
    if let Err(error) = result {
        tracing::warn!(
            source_path = %sidecar.source_path.display(),
            device_path = sidecar.device_path,
            %error,
            "could not copy lyrics sidecar to device"
        );
    }
}

/// Removes the `.lrc` that travelled with `device_path` — but only when the
/// library still holds the sidecar it was mirrored from.
///
/// `source_path` is the library file this device file came from; `None` means
/// the run cannot establish one (an orphan the inventory never recorded), and
/// then nothing is deleted. A `.lrc` on the device whose library counterpart
/// does not exist was never put there by Reprise: it is the user's own,
/// hand-authored on a player that has no internet, and it may well be the
/// only copy. Leaving a stale attachment behind is the far smaller harm.
pub(super) async fn remove_lyrics_sidecar(
    runtime: &Rc<DeviceSyncRuntime>,
    work: &PlannedWork,
    source_path: Option<&Path>,
    device_path: &str,
    target_path: &str,
    storage_id: Option<StorageId>,
) {
    if reprise_core::device_sync::lyrics_sidecar::is_sidecar_path(Path::new(device_path)) {
        return;
    }
    let Some(source_path) = source_path else {
        return;
    };
    let Some(sidecar) =
        reprise_core::device_sync::lyrics_sidecar::paths_for_track(source_path, device_path)
    else {
        return;
    };
    if reprise_core::device_sync::lyrics_sidecar::source_file_size(&sidecar.source_path).is_none() {
        return;
    }
    if let Err(error) = runtime
        .backend
        .delete_track(
            work.root_uri.clone(),
            target_path.to_string(),
            storage_id,
            sidecar.device_path.clone(),
        )
        .await
    {
        tracing::warn!(
            device_path = sidecar.device_path,
            %error,
            "could not remove lyrics sidecar from device"
        );
    }
}

pub(super) async fn remove_analysis_sidecar(
    runtime: &Rc<DeviceSyncRuntime>,
    work: &PlannedWork,
    device_path: &str,
    target_path: &str,
    storage_id: Option<StorageId>,
) {
    if reprise_core::device_sync::analysis_sidecar::is_sidecar_path(Path::new(device_path)) {
        return;
    }
    let Some(sidecar_path) =
        reprise_core::device_sync::analysis_sidecar::device_path_for_track(device_path)
    else {
        return;
    };
    if let Err(error) = runtime
        .backend
        .delete_track(
            work.root_uri.clone(),
            target_path.to_string(),
            storage_id,
            sidecar_path.clone(),
        )
        .await
    {
        tracing::warn!(device_path = sidecar_path, %error, "could not remove analysis sidecar from device");
    }
}

/// The library file the device file being replaced was mirrored from, taken
/// from the inventory row the plan carries for it.
fn replaced_source_path(work: &PlannedWork, replaced_path: &str) -> Option<PathBuf> {
    work.machine
        .borrow()
        .plan()
        .replace
        .iter()
        .find(|replacement| replacement.existing.device_path == replaced_path)
        .map(|replacement| PathBuf::from(&replacement.existing.source_path))
}

fn replacement_keeps_lyrics_sidecar(work: &PlannedWork, replaced_path: &str) -> bool {
    let Some(replaced_sidecar) =
        reprise_core::device_sync::lyrics_sidecar::device_path_for_track(replaced_path)
    else {
        return false;
    };
    work.machine
        .borrow()
        .plan()
        .replace
        .iter()
        .find(|replacement| replacement.existing.device_path == replaced_path)
        .and_then(|replacement| {
            reprise_core::device_sync::lyrics_sidecar::device_path_for_track(
                &replacement.desired.device_path,
            )
        })
        .is_some_and(|desired_sidecar| desired_sidecar == replaced_sidecar)
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
