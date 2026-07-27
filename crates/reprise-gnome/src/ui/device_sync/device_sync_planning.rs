//! Pure-data projection from library/device inventories into one sync state.

use std::rc::Rc;

use reprise_core::device_sync::podcasts::{build_plan, query_candidates, PodcastDeviceFile};
use reprise_core::device_sync::settings::{load_device_files, resolve_selection_track_ids};
use reprise_core::device_sync::transfer::build_transfer_plan_with_inventory;
use reprise_core::device_sync::{compute_delta, SyncCandidate};

use super::{build_device_tracks, DeviceSyncRuntime, PlannedSyncPhase};

impl DeviceSyncRuntime {
    pub fn recompute_delta(self: &Rc<Self>, device_id: &str) -> Result<(), String> {
        let (settings, podcast_inventory) = self
            .device_states
            .borrow()
            .iter()
            .find(|device| device.descriptor.id == device_id)
            .map(|device| {
                (
                    device.settings.clone(),
                    device
                        .contents
                        .podcast_files
                        .iter()
                        .map(|file| PodcastDeviceFile {
                            device_path: file.relative_path.clone(),
                            size_bytes: file.size_bytes,
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .ok_or_else(|| "device is not connected".to_string())?;
        let (mut delta, transfer_plan, podcast_plan, tracks) = {
            let conn = self.conn.borrow();
            let ids = resolve_selection_track_ids(&conn, &settings.selection)
                .map_err(|error| error.to_string())?;
            let tracks = reprise_core::queries::query_sync_tracks(&conn, &ids)
                .map_err(|error| error.to_string())?;
            let files = load_device_files(&conn, device_id).map_err(|error| error.to_string())?;
            let transfer_plan =
                build_transfer_plan_with_inventory(tracks, settings.opus_bitrate, &files);
            let candidates = transfer_plan
                .iter()
                .map(|entry| SyncCandidate {
                    track_id: entry.track.id,
                    device_path: entry.device_path.clone(),
                    transfer_bytes: entry.expected_bytes,
                    source_mtime: entry.track.source_mtime,
                })
                .collect::<Vec<_>>();
            let delta = compute_delta(&candidates, &files, settings.remove_deleted);
            let podcast_candidates = query_candidates(&conn).map_err(|error| error.to_string())?;
            let podcast_plan = build_plan(
                podcast_candidates,
                &podcast_inventory,
                settings.remove_deleted,
            );
            let tracks = build_device_tracks(&conn, &transfer_plan, &files, &delta);
            (delta, transfer_plan, podcast_plan, tracks)
        };
        delta.add_transfer_bytes(podcast_plan.bytes);
        if let Some(device) = self
            .device_states
            .borrow_mut()
            .iter_mut()
            .find(|device| device.descriptor.id == device_id)
        {
            device.delta = Some(delta);
            device.transfer_plan = transfer_plan;
            device.podcast_plan = podcast_plan;
            device.tracks = tracks;
            device.selected_track_count = device.transfer_plan.len();
            device.sync_phase = PlannedSyncPhase::Idle;
            device.sync_error = None;
        }
        self.notify();
        Ok(())
    }
}
