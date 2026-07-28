//! Music track transfer/removal execution for the shared Android sync
//! lifecycle — the Playlists target's counterpart to
//! `device_sync_content_transfer.rs` (podcasts/YouTube). Unlike those,
//! music follows the transfer profile (transcode-or-copy, `MTP-24`).

use super::*;

pub(super) async fn run_removals(
    runtime: &Rc<DeviceSyncRuntime>,
    work: &PlannedWork,
    deferred_replacements: &[(String, i64)],
    failures: &mut Vec<i64>,
) {
    for (index, removal) in work.plan.remove.iter().enumerate() {
        if work.cancelled.load(Ordering::SeqCst) {
            break;
        }
        let (path, track_id) = match removal {
            ManagedRemoval::Inventory(file) => (file.device_path.clone(), Some(file.track_id)),
            ManagedRemoval::Orphan(file) => (file.relative_path.clone(), None),
        };
        set_phase(
            runtime,
            &work.device_id,
            work.generation,
            syncing_phase(
                SyncStep::Removing,
                index,
                work.plan.remove.len(),
                path.clone(),
                0,
                work.plan.transfer_bytes,
            ),
        );
        match runtime
            .backend
            .delete_track(work.root_uri.clone(), work.playlists_path.clone(), path)
            .await
        {
            Ok(_) => {
                if let Some(track_id) = track_id {
                    if let Err(error) =
                        delete_device_file(&runtime.conn.borrow(), &work.device_id, track_id)
                    {
                        tracing::warn!(track_id, %error, "could not remove device inventory row");
                        failures.push(track_id);
                    }
                }
            }
            Err(error) => {
                tracing::warn!(%error, "could not remove managed device item");
                failures.push(track_id.unwrap_or(-1));
            }
        }
    }
    for (path, track_id) in deferred_replacements {
        if work.cancelled.load(Ordering::SeqCst) {
            break;
        }
        match runtime
            .backend
            .delete_track(
                work.root_uri.clone(),
                work.playlists_path.clone(),
                path.clone(),
            )
            .await
        {
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, "could not remove replaced device track");
                failures.push(*track_id);
            }
        }
    }
}

pub(super) async fn run_transfers(
    runtime: &Rc<DeviceSyncRuntime>,
    work: &PlannedWork,
    failures: &mut Vec<i64>,
) -> (Vec<(String, i64)>, u64) {
    let transfers = transfer_operations(&work.plan);
    let mut completed_bytes = 0_u64;
    let mut deferred_replacement_removals = Vec::new();
    for (token, operation) in transfers.iter().enumerate() {
        if work.cancelled.load(Ordering::SeqCst) {
            break;
        }
        let entry = &operation.desired;
        let current_track = track_activity(&entry.track.title, &entry.track.artist);
        let prepared = match entry.action {
            TransferAction::CopyOriginal => {
                Some((entry.track.source_path.clone(), entry.target_bytes, false))
            }
            action @ (TransferAction::TranscodeOpus160 | TransferAction::TranscodeMp3(_)) => {
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
                        work.plan.transfer_bytes,
                    ),
                );
                let profile =
                    transcode_profile(action).expect("transcode action must provide a profile");
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
                    Ok(file) => Some((file.path, file.size_bytes, true)),
                    Err(error) => {
                        tracing::warn!(track_id = entry.track.id, %error, "device audio transcode failed");
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
                work.plan.transfer_bytes,
            ),
        );
        let progress_runtime = Rc::downgrade(runtime);
        let progress_id = work.device_id.clone();
        let progress_generation = work.generation;
        let base = completed_bytes;
        let estimated = entry.target_bytes;
        let bytes_total = work.plan.transfer_bytes;
        let progress: Rc<dyn Fn(u64, u64)> = Rc::new(move |copied, _| {
            if let Some(runtime) = progress_runtime.upgrade() {
                update_copy_bytes(
                    &runtime,
                    &progress_id,
                    progress_generation,
                    base.saturating_add(copied.min(estimated)),
                    bytes_total,
                    copied,
                );
            }
        });
        let result = runtime
            .backend
            .replace_track(
                work.device_id.clone(),
                work.root_uri.clone(),
                work.playlists_path.clone(),
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
                let record = DeviceFileRecord {
                    device_serial: work.device_id.clone(),
                    track_id: entry.track.id,
                    source_path: entry.track.source_path.to_string_lossy().into_owned(),
                    source_size: entry.track.size_bytes,
                    source_mtime: entry.track.source_mtime,
                    device_path: entry.device_path.clone(),
                    device_size: actual_size,
                    profile_fingerprint: entry.profile_fingerprint.clone(),
                    pinned: false,
                };
                let inventory_result = {
                    let conn = runtime.conn.borrow();
                    upsert_device_file(&conn, &record)
                };
                match inventory_result {
                    Ok(()) => {
                        if let Some(old) = &operation.previous {
                            if old.device_path != entry.device_path {
                                deferred_replacement_removals
                                    .push((old.device_path.clone(), entry.track.id));
                            }
                        }
                    }
                    Err(error) => {
                        tracing::warn!(track_id = entry.track.id, %error, "could not update device inventory");
                        failures.push(entry.track.id);
                    }
                }
            }
            Err(error) => {
                tracing::warn!(track_id = entry.track.id, %error, "device transfer failed");
                if !work.cancelled.load(Ordering::SeqCst) {
                    failures.push(entry.track.id);
                }
            }
        }
        completed_bytes = completed_bytes.saturating_add(entry.target_bytes);
    }
    (deferred_replacement_removals, completed_bytes)
}
