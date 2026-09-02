use super::*;
use std::sync::atomic::Ordering;

/// Three parallel encodes yield about 0.57 seconds of CPU work per track
/// (1.70 / 3), just below the measured 0.62-second device write. That keeps
/// the device as the bottleneck while using only three of eight cores.
pub(super) const TRANSCODE_AHEAD: usize = 3;

pub(super) struct PendingTranscode {
    pub(super) handle: gtk4::glib::JoinHandle<Result<TranscodedFile, String>>,
    pub(super) cancellation: Arc<AtomicBool>,
    pub(super) staged_path: PathBuf,
}

pub(super) fn fill(runtime: &Rc<DeviceSyncRuntime>, work: &mut PlannedWork, effect: &Effect) {
    if work.cancelled.load(Ordering::SeqCst) {
        return;
    }
    let Some(from) = first_candidate(effect) else {
        return;
    };
    let candidates = work
        .machine
        .borrow()
        .transfers()
        .iter()
        .enumerate()
        .skip(from)
        .filter_map(|(index, operation)| {
            transcode_profile(operation.desired.action).map(|profile| (index, profile))
        })
        .filter(|(index, _)| {
            !work.transcode_ahead.contains_key(index) && !work.transcoded.contains_key(index)
        })
        .take(TRANSCODE_AHEAD.saturating_sub(work.transcode_ahead.len()))
        .collect::<Vec<_>>();
    for (index, profile) in candidates {
        let entry = work.machine.borrow().transfers()[index].desired.clone();
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
            metadata: reprise_platform_linux::device_transfer::AudioMetadata::for_track(
                &entry.track,
            ),
        };
        let cancellation = work.cancelled.clone();
        let future = runtime
            .backend
            .transcode_track(request, cancellation.clone());
        let handle = gtk4::glib::MainContext::ref_thread_default().spawn_local(future);
        work.transcode_ahead.insert(
            index,
            PendingTranscode {
                handle,
                cancellation,
                staged_path,
            },
        );
    }
}

fn first_candidate(effect: &Effect) -> Option<usize> {
    match effect {
        Effect::Transcode { index, .. } => Some(*index),
        Effect::CopyTrack { index, .. } | Effect::RecordFile { index, .. } => {
            Some(index.saturating_add(1))
        }
        _ => None,
    }
}

pub(super) fn cancel_all(pending: &mut HashMap<usize, PendingTranscode>) {
    for (_, transcode) in pending.drain() {
        transcode.cancellation.store(true, Ordering::SeqCst);
        transcode.handle.abort();
        reprise_core::device_sync::staging::discard(&transcode.staged_path);
    }
}

impl Drop for PlannedWork {
    fn drop(&mut self) {
        cancel_all(&mut self.transcode_ahead);
        for (_, path) in self.transcoded.drain() {
            reprise_core::device_sync::staging::discard(&path);
        }
    }
}
