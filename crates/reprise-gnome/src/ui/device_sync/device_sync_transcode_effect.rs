use super::*;
use reprise_core::device_sync::DesiredManagedFile;

pub(super) async fn perform(
    runtime: &Rc<DeviceSyncRuntime>,
    work: &mut PlannedWork,
    index: usize,
    action: TransferAction,
) -> Event {
    let Some(entry) = work.transfer(index).map(|transfer| transfer.desired) else {
        return Event::Transcoded(Err(format!(
            "the sync machine emitted invalid transfer index {index}"
        )));
    };
    let pending = work.transcode_ahead.remove(&index);
    let result = resolve(
        runtime.backend.as_ref(),
        &work.device_id,
        work.cancelled.clone(),
        pending,
        &entry,
        action,
    )
    .await;
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

async fn resolve(
    backend: &dyn DeviceBackend,
    device_id: &str,
    cancelled: Arc<AtomicBool>,
    pending: Option<transcode_prefetch::PendingTranscode>,
    entry: &DesiredManagedFile,
    action: TransferAction,
) -> Result<TranscodedFile, String> {
    if let Some(pending) = pending {
        let staged_path = pending.staged_path;
        match pending.handle.await {
            Ok(result) => finish_result(result, &staged_path),
            Err(error) => {
                reprise_core::device_sync::staging::discard(&staged_path);
                Err(format!("audio encoder task failed: {error}"))
            }
        }
    } else {
        inline(backend, device_id, cancelled, entry, action).await
    }
}

async fn inline(
    backend: &dyn DeviceBackend,
    device_id: &str,
    cancelled: Arc<AtomicBool>,
    entry: &DesiredManagedFile,
    action: TransferAction,
) -> Result<TranscodedFile, String> {
    let profile =
        transcode_profile(action).expect("a transcode effect must name a transcode action");
    let extension = match profile {
        TranscodeProfile::Opus160 => "opus",
        TranscodeProfile::Mp3(_) => "mp3",
    };
    let staged_path =
        reprise_core::device_sync::staging::temporary_path(device_id, entry.track.id, extension);
    let request = TranscodeRequest {
        source: entry.track.source_path.clone(),
        output: staged_path.clone(),
        profile,
        metadata: reprise_platform_linux::device_transfer::AudioMetadata::for_track(&entry.track),
    };
    let result = backend.transcode_track(request, cancelled).await;
    finish_result(result, &staged_path)
}

#[cfg(test)]
pub(crate) async fn without_prefetch_for_test(
    backend: &dyn DeviceBackend,
    device_id: &str,
    entry: &DesiredManagedFile,
    action: TransferAction,
) -> Result<TranscodedFile, String> {
    resolve(
        backend,
        device_id,
        Arc::new(AtomicBool::new(false)),
        None,
        entry,
        action,
    )
    .await
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
