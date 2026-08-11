use std::collections::HashSet;

use reprise_core::device_sync::settings::{
    load_device_files, load_device_playlists, resolve_selection_track_ids, save_settings,
};
use reprise_core::device_sync::{
    load_everything_playlist_snapshot, load_mirror_playlist_snapshots, project_storage,
    project_sync_page, DeviceSelection, SelectionSource, SyncPageInput, TransferProfile,
    EVERYTHING_SOURCE,
};

use super::*;

impl DeviceSyncRuntime {
    pub fn update_settings(self: &Rc<Self>, settings: DeviceSettings) -> Result<(), String> {
        let rememberable = {
            let devices = self.device_states.borrow();
            let device = devices
                .iter()
                .find(|device| device.descriptor.id == settings.device_serial)
                .ok_or_else(|| "device is not connected".to_string())?;
            if device.is_busy() {
                return Err("device synchronization is active".into());
            }
            device.descriptor.persistent_id.is_some()
        };
        if rememberable {
            save_settings(&self.conn, &settings).map_err(|error| error.to_string())?;
        }
        let device_id = settings.device_serial.clone();
        let mut devices = self.device_states.borrow_mut();
        let device = devices
            .iter_mut()
            .find(|device| device.descriptor.id == device_id)
            .ok_or_else(|| "device is not connected".to_string())?;
        device.settings = settings;
        device.sync_phase = PlannedSyncPhase::ComputingDelta;
        device.sync_error = None;
        drop(devices);
        self.recompute_delta(&device_id)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_transfer_profile(
        self: &Rc<Self>,
        device_id: &str,
        profile: TransferProfile,
    ) -> Result<(), String> {
        let mut settings = self.settings_for_update(device_id)?;
        settings.profile = profile;
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

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_remove_deleted(
        self: &Rc<Self>,
        device_id: &str,
        remove_deleted: bool,
    ) -> Result<(), String> {
        let mut settings = self.settings_for_update(device_id)?;
        settings.remove_deleted = remove_deleted;
        self.update_settings(settings)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_sync_automatically(
        self: &Rc<Self>,
        device_id: &str,
        sync_automatically: bool,
    ) -> Result<(), String> {
        let mut settings = self.settings_for_update(device_id)?;
        settings.sync_automatically = sync_automatically;
        self.update_settings(settings)
    }

    pub fn selection_options(&self) -> Result<Vec<DeviceSelectionOption>, String> {
        let conn = &self.conn;
        let mut options = reprise_core::library::playlists::list(conn)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|playlist| DeviceSelectionOption {
                source: SelectionSource::Playlist(playlist.id),
                name: playlist.name,
                track_count: usize::try_from(playlist.track_count.max(0)).unwrap_or(usize::MAX),
                smart: false,
            })
            .collect::<Vec<_>>();
        for playlist in
            reprise_core::library::playlists::list_smart(conn).map_err(|error| error.to_string())?
        {
            let source = SelectionSource::Smart(playlist.id);
            let count =
                resolve_selection_track_ids(conn, &DeviceSelection::Sources(vec![source.clone()]))
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
        let result = self.recompute_delta_silent(device_id);
        if result.is_ok() {
            self.notify();
        }
        result
    }

    pub(super) fn recompute_delta_silent(self: &Rc<Self>, device_id: &str) -> Result<(), String> {
        let (settings, storage, managed_files, managed_files_scanned) = self
            .device_states
            .borrow()
            .iter()
            .find(|device| device.descriptor.id == device_id)
            .map(|device| {
                (
                    device.settings.clone(),
                    device.storage.clone(),
                    device.managed_files.clone(),
                    device.ever_inspected && device.scan_error.is_none(),
                )
            })
            .ok_or_else(|| "device is not connected".to_string())?;
        let selected = match &settings.selection {
            DeviceSelection::Sources(sources) => sources.clone(),
            DeviceSelection::EntireLibrary => vec![EVERYTHING_SOURCE],
        };
        let keep_smart_playlists_updated =
            reprise_core::library::settings::get_bool(&self.conn, KEEP_SMART_UPDATED_KEY, true)
                .map_err(|error| error.to_string())?;
        let conn = &self.conn;
        let files = load_device_files(conn, device_id).map_err(|error| error.to_string())?;
        let managed_track_count = files.len();
        let playlist_inventory =
            load_device_playlists(conn, device_id).map_err(|error| error.to_string())?;
        let mut playlists =
            load_mirror_playlist_snapshots(conn).map_err(|error| error.to_string())?;
        if selected.contains(&EVERYTHING_SOURCE) {
            playlists
                .push(load_everything_playlist_snapshot(conn).map_err(|error| error.to_string())?);
        }
        let (frozen_smart_sources, frozen_smart_track_ids) =
            apply_frozen_smart_snapshots(conn, device_id, &selected, &mut playlists)?;
        let desktop_analyses = desktop_analysis_sizes(conn, &selected, &playlists)?;
        let published_frozen_sources = playlist_inventory
            .iter()
            .filter(|playlist| frozen_smart_sources.contains(&playlist.source))
            .map(|playlist| playlist.source.clone())
            .collect::<HashSet<_>>();
        let mut projection = project_sync_page(SyncPageInput {
            selected,
            playlists,
            profile: settings.profile,
            inventory: files,
            playlist_inventory,
            managed_files,
            managed_files_scanned,
            desktop_analyses,
            storage: storage.clone(),
        });
        reprise_core::device_sync::apply_frozen_smart_playlist_policy(
            &mut projection.plan,
            &published_frozen_sources,
            &frozen_smart_track_ids,
        );
        projection.page.changes.removals = projection.plan.remove.len();
        projection.page.changes.playlist_writes = projection.plan.playlist_writes.len();
        projection.page.changes.transfer_bytes = projection.plan.transfer_bytes;
        projection.page.blockers = projection.plan.blockers.clone();
        projection.page.storage = project_storage(&storage, &projection.plan);

        let mut devices = self.device_states.borrow_mut();
        if let Some(device) = devices
            .iter_mut()
            .find(|device| device.descriptor.id == device_id)
        {
            device.managed_track_count = managed_track_count;
            device.mirror_plan = projection.plan;
            device.keep_smart_playlists_updated = keep_smart_playlists_updated;
            device.page = projection.page;
            device.sync_phase = PlannedSyncPhase::Idle;
        }
        Ok(())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn settings_for_update(&self, device_id: &str) -> Result<DeviceSettings, String> {
        self.device_states
            .borrow()
            .iter()
            .find(|device| device.descriptor.id == device_id)
            .map(|device| device.settings.clone())
            .ok_or_else(|| "device is not connected".to_string())
    }
}

fn desktop_analysis_sizes(
    conn: &Db,
    selected: &[SelectionSource],
    playlists: &[reprise_core::device_sync::MirrorPlaylistSnapshot],
) -> Result<Vec<reprise_core::device_sync::DesktopAnalysis>, String> {
    let mut track_ids = playlists
        .iter()
        .filter(|playlist| selected.contains(&playlist.source))
        .flat_map(|playlist| &playlist.entries)
        .filter_map(|track| match track {
            reprise_core::device_sync::MirrorTrack::Available(track) => Some(track.id),
            reprise_core::device_sync::MirrorTrack::Unavailable(_) => None,
        })
        .collect::<Vec<_>>();
    track_ids.sort_unstable();
    track_ids.dedup();
    let mut analyses = Vec::new();
    for track_id in track_ids {
        let sidecar = match reprise_core::device_sync::analysis_sidecar::AnalysisSidecar::for_track(
            conn, track_id,
        ) {
            Ok(Some(sidecar)) => sidecar,
            Ok(None) => continue,
            Err(error) => {
                tracing::warn!(track_id, %error, "could not load analysis sidecar data");
                continue;
            }
        };
        let bytes = match sidecar.encode() {
            Ok(bytes) => bytes,
            Err(error) => {
                tracing::warn!(track_id, %error, "could not encode analysis sidecar data");
                continue;
            }
        };
        analyses.push(reprise_core::device_sync::DesktopAnalysis {
            track_id,
            size_bytes: u64::try_from(bytes.len())
                .map_err(|_| "analysis sidecar length does not fit u64".to_string())?,
        });
    }
    Ok(analyses)
}

pub(super) fn is_verified_track_file(file: &reprise_core::device_sync::ManagedDeviceFile) -> bool {
    let path = std::path::Path::new(&file.relative_path);
    !file.relative_path.to_ascii_lowercase().ends_with(".m3u8")
        && !reprise_core::device_sync::analysis_sidecar::is_sidecar_path(path)
        && !reprise_core::device_sync::track_metadata_list::is_list_path(path)
}

pub(super) fn verified_track_bytes(files: &[reprise_core::device_sync::ManagedDeviceFile]) -> u64 {
    files
        .iter()
        .filter(|file| is_verified_track_file(file))
        .map(|file| file.size_bytes)
        .fold(0_u64, u64::saturating_add)
}
