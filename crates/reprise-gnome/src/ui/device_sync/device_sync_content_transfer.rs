//! Shared execution for the two "always copied 1:1" content targets:
//! podcast episodes and YouTube audio (`MTP-23`, `MTP-24`). Both plan shapes
//! are the same [`PodcastSyncPlan`] (`device_sync::podcasts`), so one pair of
//! functions drives both — the only difference between a podcast-episode sync
//! step and a YouTube-audio sync step is which target path and which plan
//! slice gets passed in.
//!
//! These two targets deliberately sit **outside**
//! [`reprise_core::device_sync::DeviceSyncMachine`]. The machine reduces the
//! music and playlist mirror, whose contract is that `Music/Reprise` is
//! authoritative (`MTP-17`): it may remove any file it does not recognize. The
//! content targets are additive — they diff against their own candidate list
//! and never claim their folder — so folding them into the same reducer would
//! have meant teaching it two different notions of ownership. They run after
//! the machine reports `Finished`, and report their own phases.

use reprise_core::device_sync::podcasts::PodcastSyncCandidate;
use reprise_core::device_sync::{PlannedSyncPhase, StorageId};

use super::*;

/// One label for the row a content step is currently working on. The mirror
/// steps get theirs from the machine; these two build the same shape.
fn content_activity(title: &str, source: &str) -> String {
    if source.is_empty() {
        title.to_string()
    } else {
        format!("{title} — {source}")
    }
}

/// The total this run is transferring, for the progress bar's denominator.
/// Taken from the machine's plan so the mirror and the content targets share
/// one figure rather than each inventing its own.
fn transfer_bytes(work: &PlannedWork) -> u64 {
    work.machine.borrow().plan().transfer_bytes
}

/// Publishes a content step's phase, but only while this run is still the
/// device's current one — the same `Rc::ptr_eq` identity check the machine
/// driver uses, so a superseded run cannot write over a newer run's phase.
fn set_content_phase(runtime: &Rc<DeviceSyncRuntime>, work: &PlannedWork, phase: PlannedSyncPhase) {
    {
        let mut devices = runtime.device_states.borrow_mut();
        let Some(device) = devices
            .iter_mut()
            .find(|device| device.descriptor.id == work.device_id)
        else {
            return;
        };
        let is_current = device
            .machine
            .as_ref()
            .is_some_and(|current| Rc::ptr_eq(current, &work.machine));
        if !is_current {
            return;
        }
        device.sync_phase = phase;
    }
    runtime.notify();
}

#[allow(clippy::too_many_arguments)]
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
        bytes_done,
        bytes_total,
    }
}

/// Runs the two additive content targets after the mirror machine has
/// finished, and folds their result into its outcome (`MTP-23`).
///
/// They run last rather than interleaved because the mirror is authoritative
/// over its folder (`MTP-17`) while these two are not — letting the
/// authoritative step settle first means a content copy can never be removed
/// again by a mirror cleanup in the same run. A mirror that was cancelled or
/// failed skips the content phase entirely: the previous implementation gated
/// every later step on `failures.is_empty()` for the same reason, and a run
/// that already lost tracks should not start writing more.
pub(super) async fn run_content_phase(
    runtime: &Rc<DeviceSyncRuntime>,
    work: &mut PlannedWork,
    outcome: SyncOutcome,
) -> SyncOutcome {
    if !matches!(outcome, SyncOutcome::Completed { .. }) {
        return outcome;
    }
    if work.cancelled.load(Ordering::SeqCst) {
        return outcome;
    }
    // `MTP-46`: the plan was made when the sync started and the user may have
    // switched a source off since — `recompute_all_devices` deliberately skips
    // a device that is mid-transfer rather than interrupting it, so the stale
    // plan is still here. Re-read the switches at the moment of execution and
    // drop a disabled source's work entirely. Dropping it here rather than
    // filtering later is what keeps the *removals* out too: a stale plan's
    // `to_remove` would otherwise delete files of a source the user has just
    // taken out of the sync.
    let enabled = reprise_core::device_sync::podcasts::enabled_sync_sources(&runtime.conn.borrow())
        .unwrap_or(reprise_core::device_sync::podcasts::EnabledSyncSources {
            rss: false,
            youtube: false,
        });
    if !enabled.allows(reprise_core::device_sync::podcasts::PodcastSyncSource::Rss) {
        work.podcasts = reprise_core::device_sync::podcasts::PodcastSyncPlan::default();
    }
    if !enabled.allows(reprise_core::device_sync::podcasts::PodcastSyncSource::Youtube) {
        work.youtube = reprise_core::device_sync::podcasts::PodcastSyncPlan::default();
    }

    let mut failures = Vec::new();
    let mut copied = Vec::new();
    let mut deviations: Vec<ContentDeviation> = Vec::new();
    let copy_total = work.podcasts.to_copy.len() + work.youtube.to_copy.len();
    let podcasts = work.podcasts.clone();
    let youtube = work.youtube.clone();
    let podcasts_path = work.podcasts_path.clone();
    let youtube_path = work.youtube_path.clone();

    // Continue the mirror's progress rather than restarting it. The plan total
    // already includes the content bytes, and the mirror has just finished its
    // own share, so starting the content phase at zero would send the bar back
    // to 0% after it reached the end of step one.
    let content_bytes: u64 = podcasts
        .to_copy
        .iter()
        .chain(youtube.to_copy.iter())
        .map(|episode| episode.size_bytes)
        .sum();
    let mirror_bytes = transfer_bytes(work).saturating_sub(content_bytes);

    let mut completed = run_content_transfers(
        runtime,
        work,
        &podcasts_path,
        work.podcasts_storage,
        &podcasts.to_copy,
        0,
        copy_total,
        mirror_bytes,
        &mut failures,
        &mut copied,
        &mut deviations,
    )
    .await;
    // Every later phase is gated on the ones before it having succeeded. This
    // is not tidiness: with cap eviction (`MTP-39`/`MTP-25`) a removal is the
    // counterpart of a copy, so running removals after a failed copy can
    // delete the resident episode whose replacement never arrived. The
    // pre-machine implementation gated each step the same way, and losing that
    // guard while re-applying this phase on top of the reducer would have been
    // a data-loss regression on the device.
    if may_continue(work, &failures) {
        completed = run_content_transfers(
            runtime,
            work,
            &youtube_path,
            work.youtube_storage,
            &youtube.to_copy,
            podcasts.to_copy.len(),
            copy_total,
            completed,
            &mut failures,
            &mut copied,
            &mut deviations,
        )
        .await;
    }
    let _ = completed;

    let remove_total = podcasts.to_remove.len() + youtube.to_remove.len();
    let mut removed = 0;
    if may_continue(work, &failures) {
        removed += run_content_removals(
            runtime,
            work,
            &podcasts_path,
            work.podcasts_storage,
            &podcasts.to_remove,
            0,
            remove_total,
            &mut failures,
            &mut deviations,
        )
        .await;
    }
    if may_continue(work, &failures) {
        removed += run_content_removals(
            runtime,
            work,
            &youtube_path,
            work.youtube_storage,
            &youtube.to_remove,
            podcasts.to_remove.len(),
            remove_total,
            &mut failures,
            &mut deviations,
        )
        .await;
    }

    // The log counts what actually moved, so the content targets report
    // themselves the way the mirror's effects do (`MTP-20`) — each copy with
    // its own size, each removal once, neither estimated from the plan.
    for bytes in copied {
        work.log.copied(bytes);
    }
    for _ in 0..removed {
        work.log.deleted();
    }
    // A content failure has to reach the log like a mirror failure does,
    // otherwise the run closes claiming nothing went wrong (`MTP-20`).
    for deviation in deviations {
        work.log.note(
            runtime,
            deviation.kind,
            deviation.track_id,
            &deviation.device_path,
            deviation.detail,
        );
    }

    // A cancelled run must not close as a completed one: `finish_sync` writes
    // the log entry and clears reconnect resumability from this outcome, so
    // reporting Completed here would both misrecord the run and lose the
    // remaining work.
    if work.cancelled.load(Ordering::SeqCst) {
        return SyncOutcome::Cancelled;
    }
    if failures.is_empty() {
        outcome
    } else {
        SyncOutcome::Failed {
            terminal_error: None,
            failed_tracks: failures,
        }
    }
}

/// Whether the next content step may run: nothing cancelled, nothing failed.
fn may_continue(work: &PlannedWork, failures: &[i64]) -> bool {
    !work.cancelled.load(Ordering::SeqCst) && failures.is_empty()
}

/// One line the run log should carry for a content file that deviated
/// (`MTP-20`). Collected while the borrow of `PlannedWork` is shared and
/// written once the phase owns it mutably again.
struct ContentDeviation {
    kind: DeviationKind,
    track_id: Option<i64>,
    device_path: String,
    detail: String,
}

/// Deletes every path in `to_remove` from `target_path` on `storage_id`.
/// `offset`/`total` let the caller report progress across both content targets
/// as one combined "N of M" count rather than resetting per target. Returns
/// how many files actually left the device, for the run log (`MTP-20`).
#[allow(clippy::too_many_arguments)]
async fn run_content_removals(
    runtime: &Rc<DeviceSyncRuntime>,
    work: &PlannedWork,
    target_path: &str,
    storage_id: Option<StorageId>,
    to_remove: &[String],
    offset: usize,
    total: usize,
    failures: &mut Vec<i64>,
    deviations: &mut Vec<ContentDeviation>,
) -> usize {
    let bytes_total = transfer_bytes(work);
    let mut removed = 0_usize;
    for (index, path) in to_remove.iter().enumerate() {
        if work.cancelled.load(Ordering::SeqCst) {
            break;
        }
        set_content_phase(
            runtime,
            work,
            syncing_phase(
                SyncStep::Removing,
                offset.saturating_add(index),
                total,
                path.clone(),
                0,
                bytes_total,
            ),
        );
        match runtime
            .backend
            .delete_track(
                work.root_uri.clone(),
                target_path.to_string(),
                storage_id,
                path.clone(),
            )
            .await
        {
            // `Ok(false)` means the file was already gone, which is the
            // desired state but not a deletion this run performed — counting
            // it would inflate the run log's removal tally (`MTP-20`).
            Ok(true) => removed = removed.saturating_add(1),
            Ok(false) => {}
            Err(error) => {
                tracing::warn!(device_path = path, target_path, %error, "could not remove device content file");
                deviations.push(ContentDeviation {
                    kind: DeviationKind::Failed,
                    track_id: None,
                    device_path: path.clone(),
                    detail: format!("remove failed: {error}"),
                });
                failures.push(-1);
            }
        }
    }
    removed
}

/// Copies every candidate in `to_copy` into `target_path`, 1:1 — podcast
/// episodes and YouTube audio are already Opus or AAC, so unlike music there
/// is no transcode branch here (`MTP-24`). Returns the running
/// `completed_bytes` total so a caller chaining this across both content
/// targets keeps one continuous progress figure.
#[allow(clippy::too_many_arguments)]
async fn run_content_transfers(
    runtime: &Rc<DeviceSyncRuntime>,
    work: &PlannedWork,
    target_path: &str,
    storage_id: Option<StorageId>,
    to_copy: &[PodcastSyncCandidate],
    offset: usize,
    total: usize,
    base_bytes: u64,
    failures: &mut Vec<i64>,
    // `copied` collects the byte size of each file that actually landed, so
    // the run log counts what moved rather than what was planned (`MTP-20`).
    copied: &mut Vec<u64>,
    deviations: &mut Vec<ContentDeviation>,
) -> u64 {
    let bytes_total = transfer_bytes(work);
    let mut completed_bytes = base_bytes;
    for (index, episode) in to_copy.iter().enumerate() {
        if work.cancelled.load(Ordering::SeqCst) {
            break;
        }
        let current = content_activity(&episode.title, &episode.show);
        set_content_phase(
            runtime,
            work,
            syncing_phase(
                SyncStep::Copying,
                offset.saturating_add(index),
                total,
                current.clone(),
                completed_bytes,
                bytes_total,
            ),
        );
        let progress_runtime = Rc::downgrade(runtime);
        let machine = work.machine.clone();
        let progress_id = work.device_id.clone();
        let base = completed_bytes;
        let expected = episode.size_bytes;
        let step_current = current;
        let step_done = offset.saturating_add(index);
        let progress: Rc<dyn Fn(u64, u64)> = Rc::new(move |copied, _| {
            let Some(runtime) = progress_runtime.upgrade() else {
                return;
            };
            {
                let mut devices = runtime.device_states.borrow_mut();
                let Some(device) = devices
                    .iter_mut()
                    .find(|device| device.descriptor.id == progress_id)
                else {
                    return;
                };
                let is_current = device
                    .machine
                    .as_ref()
                    .is_some_and(|current| Rc::ptr_eq(current, &machine));
                if !is_current {
                    return;
                }
                device.sync_phase = syncing_phase(
                    SyncStep::Copying,
                    step_done,
                    total,
                    step_current.clone(),
                    base.saturating_add(copied.min(expected)),
                    bytes_total,
                );
                device.mtp_rate.observe(copied, Instant::now());
            }
            runtime.notify();
        });
        let result = runtime
            .backend
            .replace_track(
                work.device_id.clone(),
                work.root_uri.clone(),
                target_path.to_string(),
                storage_id,
                episode.source_path.clone(),
                episode.device_path.clone(),
                episode.size_bytes,
                work.cancellable.clone(),
                progress,
            )
            .await;
        match result {
            Ok(_) => copied.push(episode.size_bytes),
            Err(error) => {
                tracing::warn!(
                    episode_id = episode.episode_id,
                    target_path,
                    %error,
                    "device content transfer failed"
                );
                if !work.cancelled.load(Ordering::SeqCst) {
                    deviations.push(ContentDeviation {
                        kind: DeviationKind::Failed,
                        track_id: None,
                        device_path: episode.device_path.clone(),
                        detail: format!("copy failed: {error}"),
                    });
                    failures.push(episode.episode_id.saturating_neg());
                }
            }
        }
        completed_bytes = completed_bytes.saturating_add(episode.size_bytes);
    }
    completed_bytes
}
