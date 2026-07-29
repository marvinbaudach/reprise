//! Performs one `Effect` the core sync machine emits and answers with the
//! matching `Event` — split out of `device_sync_planned.rs` to keep that file
//! under the project's 800-line limit. Nothing here decides *what* to do next;
//! that stays `DeviceSyncMachine`'s job (see the module doc on `device_sync_planned.rs`).

use super::*;

/// Performs one effect and returns the event that answers it.
pub(super) async fn perform(
    runtime: &Rc<DeviceSyncRuntime>,
    work: &mut PlannedWork,
    effect: Effect,
) -> Event {
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
