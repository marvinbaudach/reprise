use super::*;
use reprise_core::device_sync::DesiredManagedFile;

pub(super) async fn perform(
    runtime: &Rc<DeviceSyncRuntime>,
    work: &mut PlannedWork,
    index: usize,
    action: TransferAction,
) -> Event {
    let entry = work.machine.borrow().transfers()[index].desired.clone();
    let result = if let Some(pending) = work.transcode_ahead.remove(&index) {
        let staged_path = pending.staged_path;
        match pending.handle.await {
            Ok(result) => finish_result(result, &staged_path),
            Err(error) => {
                reprise_core::device_sync::staging::discard(&staged_path);
                Err(format!("audio encoder task failed: {error}"))
            }
        }
    } else {
        inline(runtime, work, &entry, action).await
    };
    match result {
        Ok(file) => {
            let size = file.size_bytes;
            work.transcoded.insert(index, file.path);
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

async fn inline(
    runtime: &Rc<DeviceSyncRuntime>,
    work: &PlannedWork,
    entry: &DesiredManagedFile,
    action: TransferAction,
) -> Result<TranscodedFile, String> {
    let profile =
        transcode_profile(action).expect("a transcode effect must name a transcode action");
    let extension = match profile {
        TranscodeProfile::Opus160 => "opus",
        TranscodeProfile::Mp3(_) => "mp3",
    };
    let staged_path = reprise_core::device_sync::staging::temporary_path(
        &work.device_id,
        entry.track.id,
        extension,
    );
    let request = TranscodeRequest {
        source: entry.track.source_path.clone(),
        output: staged_path.clone(),
        profile,
        metadata: reprise_platform_linux::device_transfer::AudioMetadata::for_track(&entry.track),
    };
    let result = runtime
        .backend
        .transcode_track(request, work.cancelled.clone())
        .await;
    finish_result(result, &staged_path)
}

fn finish_result(
    result: Result<TranscodedFile, String>,
    staged_path: &Path,
) -> Result<TranscodedFile, String> {
    match result {
        Ok(file) => {
            if file.path != staged_path {
                reprise_core::device_sync::staging::discard(staged_path);
            }
            Ok(file)
        }
        Err(error) => {
            reprise_core::device_sync::staging::discard(staged_path);
            Err(error)
        }
    }
}
