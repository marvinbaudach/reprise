use reprise_core::device_sync::settings::{
    load_device_files, load_device_playlists, resolve_selection_track_ids, save_settings,
};
use reprise_core::device_sync::{
    load_mirror_playlist_snapshots, project_sync_page, DeviceSelection, SelectionSource,
    SyncPageInput, TransferProfile,
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
        let (projection, managed_track_count) = {
            let conn = self.conn.borrow();
            let files = load_device_files(&conn, device_id).map_err(|error| error.to_string())?;
            let managed_track_count = files.len();
            let playlist_inventory =
                load_device_playlists(&conn, device_id).map_err(|error| error.to_string())?;
            let playlists =
                load_mirror_playlist_snapshots(&conn).map_err(|error| error.to_string())?;
            (
                project_sync_page(SyncPageInput {
                    selected,
                    playlists,
                    profile: settings.profile,
                    inventory: files,
                    playlist_inventory,
                    managed_files,
                    storage,
                }),
                managed_track_count,
            )
        };
        if let Some(device) = self
            .device_states
            .borrow_mut()
            .iter_mut()
            .find(|device| device.descriptor.id == device_id)
        {
            device.managed_track_count = managed_track_count;
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
