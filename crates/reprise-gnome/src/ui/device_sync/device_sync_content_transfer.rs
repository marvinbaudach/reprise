//! Shared execution for the two "always copied 1:1" content targets:
//! podcast episodes and YouTube audio (`MTP-23`, `MTP-24`). Both plan
//! shapes are the same [`PodcastSyncPlan`] (`device_sync::podcasts`), so
//! one pair of functions drives both — the only difference between a
//! podcast-episode sync step and a YouTube-audio sync step is which target
//! path and which plan slice gets passed in.

use reprise_core::device_sync::podcasts::PodcastSyncCandidate;

use super::*;

/// Deletes every path in `to_remove` from `target_path`. `offset`/`total`
/// let the caller report progress across both content targets as one
/// combined "N of M" count rather than resetting per target.
pub(super) async fn run_content_removals(
    runtime: &Rc<DeviceSyncRuntime>,
    work: &PlannedWork,
    target_path: &str,
    to_remove: &[String],
    offset: usize,
    total: usize,
    failures: &mut Vec<i64>,
) {
    for (index, path) in to_remove.iter().enumerate() {
        if work.cancelled.load(Ordering::SeqCst) {
            break;
        }
        set_phase(
            runtime,
            &work.device_id,
            work.generation,
            syncing_phase(
                SyncStep::Removing,
                offset.saturating_add(index),
                total,
                path.clone(),
                0,
                work.plan.transfer_bytes,
            ),
        );
        if let Err(error) = runtime
            .backend
            .delete_track(work.root_uri.clone(), target_path.to_string(), path.clone())
            .await
        {
            tracing::warn!(device_path = path, target_path, %error, "could not remove device content file");
            failures.push(-1);
        }
    }
}

/// Copies every candidate in `to_copy` into `target_path`, 1:1 — podcast
/// episodes and YouTube audio are already Opus or AAC, so unlike music
/// there is no transcode branch here (`MTP-24`). Returns the running
/// `completed_bytes` total so a caller chaining this across both content
/// targets keeps one continuous progress figure.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_content_transfers(
    runtime: &Rc<DeviceSyncRuntime>,
    work: &PlannedWork,
    target_path: &str,
    to_copy: &[PodcastSyncCandidate],
    offset: usize,
    total: usize,
    base_bytes: u64,
    failures: &mut Vec<i64>,
) -> u64 {
    let mut completed_bytes = base_bytes;
    for (index, episode) in to_copy.iter().enumerate() {
        if work.cancelled.load(Ordering::SeqCst) {
            break;
        }
        let current = track_activity(&episode.title, &episode.show);
        set_phase(
            runtime,
            &work.device_id,
            work.generation,
            syncing_phase(
                SyncStep::Copying,
                offset.saturating_add(index),
                total,
                current,
                completed_bytes,
                work.plan.transfer_bytes,
            ),
        );
        let progress_runtime = Rc::downgrade(runtime);
        let progress_id = work.device_id.clone();
        let progress_generation = work.generation;
        let base = completed_bytes;
        let expected = episode.size_bytes;
        let bytes_total = work.plan.transfer_bytes;
        let progress: Rc<dyn Fn(u64, u64)> = Rc::new(move |copied, _| {
            if let Some(runtime) = progress_runtime.upgrade() {
                update_copy_bytes(
                    &runtime,
                    &progress_id,
                    progress_generation,
                    base.saturating_add(copied.min(expected)),
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
                target_path.to_string(),
                episode.source_path.clone(),
                episode.device_path.clone(),
                episode.size_bytes,
                work.cancellable.clone(),
                progress,
            )
            .await;
        if let Err(error) = result {
            tracing::warn!(
                episode_id = episode.episode_id,
                target_path,
                %error,
                "device content transfer failed"
            );
            if !work.cancelled.load(Ordering::SeqCst) {
                failures.push(episode.episode_id.saturating_neg());
            }
        }
        completed_bytes = completed_bytes.saturating_add(episode.size_bytes);
    }
    completed_bytes
}
