use super::*;

pub(super) async fn run_work(
    weak: Weak<DeviceSyncRuntime>,
    device_id: String,
    generation: u64,
    mut work: Work,
    cancellable: gio::Cancellable,
) {
    while work.next_track < work.job.tracks.len() {
        let Some(runtime) = weak.upgrade() else {
            return;
        };
        if cancellable.is_cancelled() {
            finish_interrupted(&runtime, &device_id, generation, work);
            return;
        }
        let track = work.job.tracks[work.next_track].clone();
        let root_uri = {
            let mut states = runtime.device_states.borrow_mut();
            let Some(device) = states
                .iter_mut()
                .find(|device| device.descriptor.id == device_id)
            else {
                return;
            };
            if device.generation != generation || !device.connected {
                None
            } else {
                device
                    .queue
                    .begin_track(&track.original_name, Some(track.size_bytes));
                Some(device.descriptor.root_uri.clone())
            }
        };
        let Some(root_uri) = root_uri else {
            finish_interrupted(&runtime, &device_id, generation, work);
            return;
        };
        runtime.notify();
        let relative_target = track_relative_path(&work.job.playlist, &track);
        let progress_runtime = Rc::downgrade(&runtime);
        let progress_id = device_id.clone();
        let progress: Rc<dyn Fn(u64, u64)> = Rc::new(move |copied, _total| {
            let Some(runtime) = progress_runtime.upgrade() else {
                return;
            };
            let updated = {
                let mut states = runtime.device_states.borrow_mut();
                let Some(device) = states
                    .iter_mut()
                    .find(|device| device.descriptor.id == progress_id)
                else {
                    return;
                };
                if device.generation != generation {
                    return;
                }
                device.queue.set_track_bytes(copied);
                true
            };
            if updated {
                runtime.notify();
            }
        });
        let result = runtime
            .backend
            .copy_track(
                device_id.clone(),
                root_uri,
                track.source_path.clone(),
                relative_target.clone(),
                track.size_bytes,
                cancellable.clone(),
                progress,
            )
            .await;
        if cancellable.is_cancelled() {
            finish_interrupted(&runtime, &device_id, generation, work);
            return;
        }
        match result {
            Ok(outcome) => {
                if let Some(device) = runtime
                    .device_states
                    .borrow_mut()
                    .iter_mut()
                    .find(|device| device.descriptor.id == device_id)
                {
                    if device.generation != generation {
                        return;
                    }
                    device.reserved_bytes = device.reserved_bytes.saturating_sub(track.size_bytes);
                    if outcome == CopyOutcome::Copied {
                        device.available_bytes = device
                            .available_bytes
                            .map(|available| available.saturating_sub(track.size_bytes));
                    }
                    device.queue.set_track_bytes(track.size_bytes);
                    device.queue.finish_track(match outcome {
                        CopyOutcome::Copied => TrackOutcome::Copied,
                        CopyOutcome::Skipped => TrackOutcome::Skipped,
                    });
                }
                work.appended.push(export_entry(&track, relative_target));
            }
            Err(error) => {
                tracing::warn!(device_id, %error, "device track copy failed");
                if let Some(device) = runtime
                    .device_states
                    .borrow_mut()
                    .iter_mut()
                    .find(|device| device.descriptor.id == device_id)
                {
                    if device.generation != generation {
                        return;
                    }
                    device.reserved_bytes = device.reserved_bytes.saturating_sub(track.size_bytes);
                    device.queue.finish_track(TrackOutcome::Failed);
                }
            }
        }
        work.next_track += 1;
        runtime.notify();
    }
    finish_playlist(&weak, &device_id, generation, work, cancellable).await;
}

fn finish_interrupted(
    runtime: &Rc<DeviceSyncRuntime>,
    device_id: &str,
    generation: u64,
    work: Work,
) {
    let remaining_bytes = remaining_work_bytes(&work);
    let mut continue_queue = false;
    if let Some(device) = runtime
        .device_states
        .borrow_mut()
        .iter_mut()
        .find(|device| device.descriptor.id == device_id)
    {
        if device.generation != generation {
            return;
        }
        device.running = false;
        device.cancellable = None;
        if device.interrupted_disconnect && device.descriptor.reconnectable {
            device.paused_work = Some(work);
            device.queue.pause_disconnected();
            continue_queue = device.connected;
        } else if device.interrupted_disconnect {
            device.reserved_bytes = device.reserved_bytes.saturating_sub(remaining_bytes);
            device
                .queue
                .fail_job("Device disconnected; reconnect and enqueue again");
        } else {
            device.reserved_bytes = device.reserved_bytes.saturating_sub(remaining_bytes);
            device.queue.finish_job();
            continue_queue = device.connected;
        }
        device.interrupted_disconnect = false;
    }
    runtime.notify();
    runtime.release_and_start_next(device_id);
    if continue_queue && runtime.active_device.borrow().is_none() {
        runtime.start_or_resume(device_id);
    }
}

pub(super) fn remaining_work_bytes(work: &Work) -> u64 {
    work.job.tracks[work.next_track..]
        .iter()
        .fold(0_u64, |total, track| total.saturating_add(track.size_bytes))
}

async fn finish_playlist(
    weak: &Weak<DeviceSyncRuntime>,
    device_id: &str,
    generation: u64,
    work: Work,
    cancellable: gio::Cancellable,
) {
    let Some(runtime) = weak.upgrade() else {
        return;
    };
    let interrupted = runtime
        .device_states
        .borrow()
        .iter()
        .find(|device| device.descriptor.id == device_id)
        .is_none_or(|device| {
            !device.connected || device.interrupted_disconnect || cancellable.is_cancelled()
        });
    if interrupted {
        finish_interrupted(&runtime, device_id, generation, work);
        return;
    }
    let root_uri = {
        let states = runtime.device_states.borrow();
        let Some(device) = states
            .iter()
            .find(|device| device.descriptor.id == device_id)
        else {
            return;
        };
        device.descriptor.root_uri.clone()
    };
    let result = async {
        let existing = runtime
            .backend
            .read_playlist(root_uri.clone(), work.job.playlist.clone())
            .await?;
        let contents = merge_playlist_entries(&existing, &work.appended).into_bytes();
        runtime
            .backend
            .replace_playlist(
                device_id.to_string(),
                root_uri,
                work.job.playlist.clone(),
                contents,
            )
            .await
    }
    .await;
    let interrupted = runtime
        .device_states
        .borrow()
        .iter()
        .find(|device| device.descriptor.id == device_id)
        .is_some_and(|device| device.interrupted_disconnect || cancellable.is_cancelled());
    if interrupted {
        finish_interrupted(&runtime, device_id, generation, work);
        return;
    }
    if let Some(device) = runtime
        .device_states
        .borrow_mut()
        .iter_mut()
        .find(|device| device.descriptor.id == device_id)
    {
        if device.generation != generation {
            return;
        }
        device.running = false;
        device.cancellable = None;
        match result {
            Ok(()) => device.queue.finish_job(),
            Err(error) => device.queue.fail_job(error),
        }
    }
    runtime.notify();
    runtime.refresh_contents(device_id);
    runtime.release_and_start_next(device_id);
}

fn export_entry(track: &SyncTrack, path: String) -> M3uExportEntry {
    let display = if track.artist.trim().is_empty() {
        track.title.clone()
    } else {
        format!("{} - {}", track.artist, track.title)
    };
    M3uExportEntry {
        path,
        duration_secs: track.duration_ms.max(0) / 1_000,
        display,
    }
}
