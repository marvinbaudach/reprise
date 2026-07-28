use reprise_core::device_sync::podcasts::{
    build_plan as build_podcast_plan, query_candidates_for_device, PodcastDeviceFile,
    PodcastSyncSource,
};
use reprise_core::device_sync::settings::{
    load_device_files, load_device_playlists, resolve_selection_track_ids, save_settings,
};
use reprise_core::device_sync::{
    load_mirror_playlist_snapshots, load_or_create_targets, project_storage, project_sync_page,
    DeviceSelection, SelectionSource, SyncPageInput, SyncTargetKind, TransferProfile,
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
            if device.is_busy() {
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
        let result = self.recompute_delta_silent(device_id);
        if result.is_ok() {
            self.notify();
        }
        result
    }

    pub(super) fn recompute_delta_silent(self: &Rc<Self>, device_id: &str) -> Result<(), String> {
        let (settings, storage, managed_files, podcast_files, youtube_files) = self
            .device_states
            .borrow()
            .iter()
            .find(|device| device.descriptor.id == device_id)
            .map(|device| {
                (
                    device.settings.clone(),
                    device.storage.clone(),
                    device.managed_files.clone(),
                    device.podcast_files.clone(),
                    device.youtube_files.clone(),
                )
            })
            .ok_or_else(|| "device is not connected".to_string())?;
        let selected = match &settings.selection {
            DeviceSelection::Sources(sources) => sources.clone(),
            DeviceSelection::EntireLibrary => Vec::new(),
        };
        let (mut projection, podcast_plan, youtube_plan, managed_track_count) = {
            let conn = self.conn.borrow();
            let files = load_device_files(&conn, device_id).map_err(|error| error.to_string())?;
            let managed_track_count = files.len();
            let playlist_inventory =
                load_device_playlists(&conn, device_id).map_err(|error| error.to_string())?;
            let playlists =
                load_mirror_playlist_snapshots(&conn).map_err(|error| error.to_string())?;
            let projection = project_sync_page(SyncPageInput {
                selected,
                playlists,
                profile: settings.profile,
                inventory: files,
                playlist_inventory,
                managed_files,
                storage: storage.clone(),
            });
            let targets =
                load_or_create_targets(&conn, device_id).map_err(|error| error.to_string())?;
            let podcast_inventory = as_podcast_device_files(&podcast_files);
            let youtube_inventory = as_podcast_device_files(&youtube_files);
            // Both kinds are queried once and each `build_plan` call filters
            // by its own `PodcastSyncSource` — the same candidate set feeds
            // both target plans, mirroring how RSS and YouTube are equally
            // eligible for phone sync (`POD-12`).
            let candidates =
                query_candidates_for_device(&conn, device_id).map_err(|error| error.to_string())?;
            let podcast_plan = target_podcast_plan(
                &targets,
                SyncTargetKind::PodcastEpisodes,
                candidates.clone(),
                &podcast_inventory,
                PodcastSyncSource::Rss,
            );
            let youtube_plan = target_podcast_plan(
                &targets,
                SyncTargetKind::YoutubeAudio,
                candidates,
                &youtube_inventory,
                PodcastSyncSource::Youtube,
            );
            (projection, podcast_plan, youtube_plan, managed_track_count)
        };
        projection.plan.transfer_bytes = projection
            .plan
            .transfer_bytes
            .saturating_add(podcast_plan.bytes)
            .saturating_add(youtube_plan.bytes);
        if podcast_plan.selected > 0 || youtube_plan.selected > 0 {
            projection.plan.blockers.retain(|blocker| {
                blocker != &reprise_core::device_sync::MirrorBlocker::NoPlaylistsSelected
            });
        }
        projection.page.changes.transfer_bytes = projection.plan.transfer_bytes;
        projection.page.blockers = projection.plan.blockers.clone();
        projection.page.storage = project_storage(&storage, &projection.plan);
        if let Some(device) = self
            .device_states
            .borrow_mut()
            .iter_mut()
            .find(|device| device.descriptor.id == device_id)
        {
            device.managed_track_count = managed_track_count;
            device.mirror_plan = projection.plan;
            device.podcast_plan = podcast_plan;
            device.youtube_plan = youtube_plan;
            device.page = projection.page;
            device.sync_phase = PlannedSyncPhase::Idle;
        }
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

/// Builds one target's podcast/YouTube plan, or an empty one when that
/// target is switched off for this device (`SyncTarget::enabled`) — a
/// disabled target has no active slot for its category regardless of what
/// candidates or inventory exist.
fn target_podcast_plan(
    targets: &[reprise_core::device_sync::SyncTarget; 3],
    kind: SyncTargetKind,
    candidates: Vec<reprise_core::device_sync::podcasts::PodcastSyncCandidate>,
    inventory: &[PodcastDeviceFile],
    source: PodcastSyncSource,
) -> reprise_core::device_sync::podcasts::PodcastSyncPlan {
    let Some(target) = targets.iter().find(|target| target.kind == kind) else {
        return reprise_core::device_sync::podcasts::PodcastSyncPlan::default();
    };
    if !target.enabled {
        return reprise_core::device_sync::podcasts::PodcastSyncPlan::default();
    }
    build_podcast_plan(candidates, inventory, true, source, target.cap_bytes)
}

fn as_podcast_device_files(files: &[ManagedDeviceFile]) -> Vec<PodcastDeviceFile> {
    files
        .iter()
        .map(|file| PodcastDeviceFile {
            device_path: file.relative_path.clone(),
            size_bytes: file.size_bytes,
        })
        .collect()
}
