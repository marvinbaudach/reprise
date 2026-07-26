use std::collections::{HashMap, HashSet};

use reprise_core::device_sync::settings::{
    load_device_files, load_device_playlists, resolve_selection_track_ids, save_settings,
};
use reprise_core::device_sync::transfer::{TransferMode, TransferPlanEntry};
use reprise_core::device_sync::{
    load_mirror_playlist_snapshots, project_sync_page, DesiredManagedFile, DeviceSelection,
    ManagedRemoval, Mp3Quality, SelectionSource, SyncDelta, SyncPageInput, TransferAction,
    TransferProfile,
};

use super::*;

impl DeviceSyncRuntime {
    pub fn update_settings(self: &Rc<Self>, settings: DeviceSettings) -> Result<(), String> {
        {
            let devices = self.device_states.borrow();
            let device = devices
                .iter()
                .find(|device| device.descriptor.id == settings.device_serial)
                .ok_or_else(|| "device is not connected".to_string())?;
            if device.is_active() {
                return Err("device synchronization is active".into());
            }
        }
        save_settings(&self.conn.borrow(), &settings).map_err(|error| error.to_string())?;
        let device_id = settings.device_serial.clone();
        {
            let mut devices = self.device_states.borrow_mut();
            let Some(device) = devices
                .iter_mut()
                .find(|device| device.descriptor.id == device_id)
            else {
                return Err("device is not connected".into());
            };
            device.settings = settings;
            device.sync_phase = PlannedSyncPhase::ComputingDelta;
            device.sync_error = None;
        }
        self.recompute_delta(&device_id)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_mp3_quality(
        self: &Rc<Self>,
        device_id: &str,
        quality: Mp3Quality,
    ) -> Result<(), String> {
        let mut settings = self.settings_for_update(device_id)?;
        settings.profile = TransferProfile::Mp3(quality);
        self.update_settings(settings)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_playlist_selected(
        self: &Rc<Self>,
        device_id: &str,
        source: SelectionSource,
        selected: bool,
    ) -> Result<(), String> {
        let mut settings = self.settings_for_update(device_id)?;
        let mut sources = match settings.selection {
            DeviceSelection::Sources(sources) => sources,
            DeviceSelection::EntireLibrary => Vec::new(),
        };
        sources.retain(|candidate| candidate != &source);
        if selected {
            sources.push(source);
        }
        settings.selection = DeviceSelection::Sources(sources);
        self.update_settings(settings)
    }

    pub fn selection_options(&self) -> Result<Vec<DeviceSelectionOption>, String> {
        let conn = self.conn.borrow();
        let mut options = reprise_core::library::playlists::list(&conn)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|playlist| DeviceSelectionOption {
                source: SelectionSource::Playlist(playlist.id),
                name: playlist.name,
                track_count: usize::try_from(playlist.track_count.max(0)).unwrap_or(usize::MAX),
                smart: false,
            })
            .collect::<Vec<_>>();
        for playlist in reprise_core::library::playlists::list_smart(&conn)
            .map_err(|error| error.to_string())?
        {
            let source = SelectionSource::Smart(playlist.id);
            let count =
                resolve_selection_track_ids(&conn, &DeviceSelection::Sources(vec![source.clone()]))
                    .map_err(|error| error.to_string())?
                    .len();
            options.push(DeviceSelectionOption {
                source,
                name: playlist.name,
                track_count: count,
                smart: true,
            });
        }
        Ok(options)
    }

    pub fn recompute_delta(self: &Rc<Self>, device_id: &str) -> Result<(), String> {
        let (settings, storage, managed_files) = self
            .device_states
            .borrow()
            .iter()
            .find(|device| device.descriptor.id == device_id)
            .map(|device| {
                (
                    device.settings.clone(),
                    device.storage.clone(),
                    device.managed_files.clone(),
                )
            })
            .ok_or_else(|| "device is not connected".to_string())?;
        let selected = match &settings.selection {
            DeviceSelection::Sources(sources) => sources.clone(),
            DeviceSelection::EntireLibrary => Vec::new(),
        };
        let (projection, files) = {
            let conn = self.conn.borrow();
            let files = load_device_files(&conn, device_id).map_err(|error| error.to_string())?;
            let playlist_inventory =
                load_device_playlists(&conn, device_id).map_err(|error| error.to_string())?;
            let playlists =
                load_mirror_playlist_snapshots(&conn).map_err(|error| error.to_string())?;
            (
                project_sync_page(SyncPageInput {
                    selected,
                    playlists,
                    profile: settings.profile,
                    inventory: files.clone(),
                    playlist_inventory,
                    managed_files,
                    storage,
                }),
                files,
            )
        };
        let transfer_plan = projection
            .plan
            .desired_files
            .iter()
            .map(legacy_transfer)
            .collect::<Vec<_>>();
        let delta = legacy_delta(&projection.plan);
        let tracks = build_device_tracks(&self.conn.borrow(), &transfer_plan, &files, &delta);
        if let Some(device) = self
            .device_states
            .borrow_mut()
            .iter_mut()
            .find(|device| device.descriptor.id == device_id)
        {
            device.delta = Some(delta);
            device.transfer_plan = transfer_plan;
            device.tracks = tracks;
            device.selected_track_count = projection.page.unique_track_count;
            device.mirror_plan = projection.plan;
            device.page = projection.page;
            device.sync_phase = PlannedSyncPhase::Idle;
        }
        self.notify();
        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn settings_for_update(&self, device_id: &str) -> Result<DeviceSettings, String> {
        self.device_states
            .borrow()
            .iter()
            .find(|device| device.descriptor.id == device_id)
            .map(|device| device.settings.clone())
            .ok_or_else(|| "device is not connected".to_string())
    }
}

fn legacy_transfer(file: &DesiredManagedFile) -> TransferPlanEntry {
    TransferPlanEntry {
        track: file.track.clone(),
        device_path: file.device_path.clone(),
        expected_bytes: file.target_bytes,
        mode: match file.action {
            TransferAction::CopyOriginal => TransferMode::Copy,
            TransferAction::TranscodeMp3(quality) => TransferMode::TranscodeMp3 { quality },
        },
    }
}

fn legacy_delta(plan: &MirrorPlan) -> SyncDelta {
    let mut to_copy = plan
        .copy
        .iter()
        .map(|file| file.track.id)
        .chain(
            plan.replace
                .iter()
                .map(|replacement| replacement.desired.track.id),
        )
        .collect::<Vec<_>>();
    to_copy.sort_unstable();
    to_copy.dedup();
    let mut to_remove = plan
        .remove
        .iter()
        .filter_map(|removal| match removal {
            ManagedRemoval::Inventory(file) => Some(file.track_id),
            ManagedRemoval::Orphan(_) => None,
        })
        .collect::<Vec<_>>();
    to_remove.sort_unstable();
    to_remove.dedup();
    SyncDelta {
        to_copy,
        to_remove,
        bytes: plan.transfer_bytes,
        est_secs: plan
            .transfer_bytes
            .div_ceil(5 * 1_024 * 1_024)
            .min(u64::from(u32::MAX)) as u32,
    }
}

fn build_device_tracks(
    conn: &Connection,
    transfer_plan: &[TransferPlanEntry],
    files: &[reprise_core::device_sync::DeviceFileRecord],
    delta: &SyncDelta,
) -> Vec<DeviceTrackView> {
    let files_by_id = files
        .iter()
        .map(|file| (file.track_id, file))
        .collect::<HashMap<_, _>>();
    let selected = transfer_plan
        .iter()
        .map(|entry| entry.track.id)
        .collect::<HashSet<_>>();
    let queued = delta.to_copy.iter().copied().collect::<HashSet<_>>();
    let removing = delta.to_remove.iter().copied().collect::<HashSet<_>>();
    let mut tracks = transfer_plan
        .iter()
        .map(|entry| {
            let file = files_by_id.get(&entry.track.id);
            DeviceTrackView {
                track_id: entry.track.id,
                title: entry.track.title.clone(),
                artist: entry.track.artist.clone(),
                device_path: entry.device_path.clone(),
                size: entry.expected_bytes,
                duration_ms: entry.track.duration_ms,
                status: if queued.contains(&entry.track.id) {
                    DeviceTrackStatus::Queued
                } else {
                    DeviceTrackStatus::Synced
                },
                pinned: file.is_some_and(|file| file.pinned),
            }
        })
        .collect::<Vec<_>>();
    for file in files
        .iter()
        .filter(|file| !selected.contains(&file.track_id))
    {
        let metadata = conn
            .query_row(
                "SELECT title, artist, duration_ms FROM tracks WHERE id = ?1",
                [file.track_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap_or_else(|_| (file.device_path.clone(), String::new(), 0));
        tracks.push(DeviceTrackView {
            track_id: file.track_id,
            title: metadata.0,
            artist: metadata.1,
            device_path: file.device_path.clone(),
            size: file.device_size,
            duration_ms: metadata.2,
            status: if removing.contains(&file.track_id) {
                DeviceTrackStatus::Remove
            } else {
                DeviceTrackStatus::Synced
            },
            pinned: file.pinned,
        });
    }
    tracks.sort_by(|left, right| left.title.cmp(&right.title));
    tracks
}
