//! Live, path-free bridge between the GTK-owned sync runtime and local agents.

use reprise_core::agent_device_sync::{
    AgentDeviceSyncCommand, AgentDeviceSyncDevice, AgentDeviceSyncPhase, AgentDeviceSyncRequest,
    AgentDeviceSyncState, SharedAgentDeviceSyncState,
};
use reprise_core::device_sync::{Mp3Quality, TransferProfile};

use super::*;

impl DeviceSyncRuntime {
    pub fn bind_agent_device_sync(
        self: &Rc<Self>,
        shared: &SharedAgentDeviceSyncState,
        commands: async_channel::Receiver<AgentDeviceSyncRequest>,
    ) {
        let mirror = shared.clone();
        let subscription = self.subscribe(Rc::new(move |state| {
            *mirror
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = agent_state(state);
        }));
        self.agent_subscription.replace(Some(subscription));

        let weak = Rc::downgrade(self);
        gtk4::glib::MainContext::ref_thread_default().spawn_local(async move {
            while let Ok(request) = commands.recv().await {
                let Some(runtime) = weak.upgrade() else {
                    break;
                };
                let result = runtime.apply_agent_command(request.command);
                let _ = request.reply.send(result);
            }
        });
    }

    fn apply_agent_command(self: &Rc<Self>, command: AgentDeviceSyncCommand) -> Result<(), String> {
        match command {
            AgentDeviceSyncCommand::ConfigurePlaylist {
                device_name,
                playlist_name,
                remove_unselected,
                bitrate_kbps,
            } => {
                let mut settings = self
                    .unique_connected_device(&device_name)
                    .map(|device| device.settings)
                    .ok_or_else(|| {
                        format!("device '{device_name}' is absent, disconnected, or ambiguous")
                    })?;
                let options = self
                    .selection_options()
                    .map_err(|error| format!("could not resolve sync playlist: {error}"))?;
                let mut matches = options
                    .into_iter()
                    .filter(|option| option.name == playlist_name);
                let option = matches
                    .next()
                    .ok_or_else(|| format!("playlist '{playlist_name}' does not exist"))?;
                if matches.next().is_some() {
                    return Err(format!("playlist name '{playlist_name}' is ambiguous"));
                }
                let quality =
                    Mp3Quality::try_from(bitrate_kbps).map_err(|error| error.to_string())?;
                settings.selection = DeviceSelection::Sources(vec![option.source]);
                settings.remove_deleted = remove_unselected;
                settings.profile = TransferProfile::Mp3(quality);
                settings.opus_bitrate = 0;
                self.update_settings(settings)
            }
            AgentDeviceSyncCommand::Start { device_name } => {
                let device_id = self
                    .unique_connected_device(&device_name)
                    .map(|device| device.id)
                    .ok_or_else(|| {
                        format!("device '{device_name}' is absent, disconnected, or ambiguous")
                    })?;
                self.sync_now(&device_id).map_err(|error| error.to_string())
            }
            AgentDeviceSyncCommand::Cancel { device_name } => {
                let device_id = self
                    .unique_connected_device(&device_name)
                    .map(|device| device.id)
                    .ok_or_else(|| {
                        format!("device '{device_name}' is absent, disconnected, or ambiguous")
                    })?;
                self.cancel_current(&device_id);
                Ok(())
            }
        }
    }

    fn unique_connected_device(&self, name: &str) -> Option<DeviceView> {
        let devices = self
            .devices()
            .into_iter()
            .filter(|device| device.connected && device.name == name)
            .collect::<Vec<_>>();
        (devices.len() == 1).then(|| devices[0].clone())
    }
}

fn agent_state(state: DeviceSyncState) -> AgentDeviceSyncState {
    AgentDeviceSyncState {
        devices: state.devices.into_iter().map(agent_device).collect(),
    }
}

fn agent_device(device: DeviceView) -> AgentDeviceSyncDevice {
    let (phase, bytes_done, bytes_total, current_track) = agent_phase(device.sync_phase);
    AgentDeviceSyncDevice {
        name: device.name,
        connected: device.connected,
        available_bytes: device.storage.free_bytes,
        total_bytes: device.storage.total_bytes,
        managed_tracks: device.managed_track_count,
        selected_tracks: device.page.unique_track_count,
        tracks_to_copy: device.page.changes.additions + device.page.changes.replacements,
        tracks_to_remove: device.page.changes.removals,
        bytes_to_copy: device.page.changes.transfer_bytes,
        phase,
        bytes_done,
        bytes_total,
        bytes_per_second: device.bytes_per_second,
        current_track,
    }
}

fn agent_phase(phase: PlannedSyncPhase) -> (AgentDeviceSyncPhase, u64, u64, String) {
    match phase {
        PlannedSyncPhase::Idle => (AgentDeviceSyncPhase::Idle, 0, 0, String::new()),
        PlannedSyncPhase::ComputingDelta => {
            (AgentDeviceSyncPhase::ComputingDelta, 0, 0, String::new())
        }
        PlannedSyncPhase::Finishing => (AgentDeviceSyncPhase::Finishing, 0, 0, String::new()),
        PlannedSyncPhase::Syncing {
            step,
            current_track,
            bytes_done,
            bytes_total,
            ..
        } => (
            match step {
                SyncStep::Removing => AgentDeviceSyncPhase::Removing,
                SyncStep::Transcoding => AgentDeviceSyncPhase::Transcoding,
                SyncStep::Copying => AgentDeviceSyncPhase::Copying,
                SyncStep::WritingPlaylists => AgentDeviceSyncPhase::WritingPlaylists,
            },
            bytes_done,
            bytes_total,
            current_track,
        ),
    }
}
