//! The two effects a CUE file adds to a run (CUE-15): the inventory row of a
//! track that shares another track's file, and the sheet derived for the
//! device beside that file. Split out of `device_sync_effects.rs` to keep it
//! under the project's 800-line limit.

use super::*;

/// Records `plan.shared_records[index]` against the file at `device_path`.
pub(super) fn record_shared_file(
    runtime: &Rc<DeviceSyncRuntime>,
    work: &PlannedWork,
    index: usize,
    device_size: u64,
    device_path: String,
) -> Event {
    if !work.persist_device_state {
        return Event::SharedFileRecorded(Ok(()));
    }
    let entry = work.machine.borrow().plan().shared_records[index]
        .desired
        .clone();
    let record = DeviceFileRecord {
        device_serial: work.device_id.clone(),
        track_id: entry.track.id,
        source_path: entry.track.source_path.to_string_lossy().into_owned(),
        source_size: entry.track.size_bytes,
        source_mtime: entry.track.source_mtime,
        device_path,
        device_size,
        profile_fingerprint: entry.profile_fingerprint.clone(),
        pinned: false,
    };
    let result = upsert_device_file(&runtime.conn, &record);
    Event::SharedFileRecorded(result.map_err(|error| {
        tracing::warn!(track_id = entry.track.id, %error, "could not record a CUE track's shared file");
        error.to_string()
    }))
}

/// Writes `plan.cue_writes[index]` beside its audio on the device. The bytes
/// come from the plan; nothing is read from or written to the music collection.
pub(super) async fn write_derived_cue(
    runtime: &Rc<DeviceSyncRuntime>,
    work: &mut PlannedWork,
    index: usize,
) -> Event {
    let planned = work.machine.borrow().plan().cue_writes[index].clone();
    let bytes = planned.contents.into_bytes();
    let temporary_path = match reprise_core::device_sync::staging::stage_bytes(
        &work.device_id,
        planned.track_id,
        "derived-cue",
        &bytes,
    ) {
        Ok(path) => path,
        Err(error) => {
            tracing::warn!(track_id = planned.track_id, %error, "could not stage a derived CUE sheet");
            return Event::DerivedCueWritten(Err(error.to_string()));
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
    match result {
        Ok(_) => {
            work.log.copied(bytes.len() as u64);
            Event::DerivedCueWritten(Ok(bytes.len() as u64))
        }
        Err(error) => {
            tracing::warn!(device_path = planned.device_path, %error, "could not write a derived CUE sheet");
            work.log.note(
                runtime,
                DeviationKind::Failed,
                Some(planned.track_id),
                &planned.device_path,
                format!("CUE sheet write failed: {error}"),
            );
            Event::DerivedCueWritten(Err(error))
        }
    }
}
