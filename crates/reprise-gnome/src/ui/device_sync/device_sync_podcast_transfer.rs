//! Podcast-specific execution for the shared Android sync lifecycle.

use super::*;

pub(super) async fn run_podcast_removals(
    runtime: &Rc<DeviceSyncRuntime>,
    work: &PlannedWork,
    failures: &mut Vec<i64>,
) {
    let music_count = work.plan.remove.len();
    let total = music_count.saturating_add(work.podcasts.to_remove.len());
    for (index, path) in work.podcasts.to_remove.iter().enumerate() {
        if work.cancelled.load(Ordering::SeqCst) {
            break;
        }
        set_phase(
            runtime,
            &work.device_id,
            work.generation,
            syncing_phase(
                SyncStep::Removing,
                music_count.saturating_add(index),
                total,
                path.clone(),
                0,
                work.plan.transfer_bytes,
            ),
        );
        if let Err(error) = runtime
            .backend
            .delete_managed(work.root_uri.clone(), ManagedRoot::Podcasts, path.clone())
            .await
        {
            tracing::warn!(device_path = path, %error, "could not remove device podcast");
            failures.push(-1);
        }
    }
}

pub(super) async fn run_podcast_transfers(
    runtime: &Rc<DeviceSyncRuntime>,
    work: &PlannedWork,
    base_bytes: u64,
    failures: &mut Vec<i64>,
) {
    let mut completed_bytes = base_bytes;
    for (index, episode) in work.podcasts.to_copy.iter().enumerate() {
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
                index,
                work.podcasts.to_copy.len(),
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
            .replace_managed(
                work.device_id.clone(),
                work.root_uri.clone(),
                ManagedRoot::Podcasts,
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
                %error,
                "device podcast transfer failed"
            );
            if !work.cancelled.load(Ordering::SeqCst) {
                failures.push(episode.episode_id.saturating_neg());
            }
        }
        completed_bytes = completed_bytes.saturating_add(episode.size_bytes);
    }
}
