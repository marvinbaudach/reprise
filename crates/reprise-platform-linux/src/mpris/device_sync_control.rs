//! Reprise-specific local D-Bus surface for live device synchronization.

use zbus::interface;

use reprise_core::agent_device_sync::{
    agent_device_sync_request, read_agent_device_sync_state, AgentDeviceSyncCommand,
    AgentDeviceSyncPhase, AgentDeviceSyncRequest, SharedAgentDeviceSyncState,
};

pub(super) type DeviceSyncRow = (
    String,
    bool,
    bool,
    u64,
    bool,
    u64,
    u64,
    u64,
    u64,
    u64,
    u64,
    String,
    u64,
    u64,
    u64,
    String,
);

pub(super) struct DeviceSyncControl {
    commands: async_channel::Sender<AgentDeviceSyncRequest>,
    state: SharedAgentDeviceSyncState,
}

impl DeviceSyncControl {
    pub(super) fn new(
        commands: async_channel::Sender<AgentDeviceSyncRequest>,
        state: SharedAgentDeviceSyncState,
    ) -> Self {
        Self { commands, state }
    }

    fn dispatch(&self, command: AgentDeviceSyncCommand) -> zbus::fdo::Result<()> {
        let (request, reply) = agent_device_sync_request(command);
        self.commands.try_send(request).map_err(|error| {
            zbus::fdo::Error::Failed(format!(
                "device sync request was not accepted by the UI: {error}"
            ))
        })?;
        match reply.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(message)) => Err(zbus::fdo::Error::Failed(message)),
            Err(error) => Err(zbus::fdo::Error::Failed(format!(
                "device sync UI did not confirm the request: {error}"
            ))),
        }
    }
}

#[interface(name = "org.reprise.DeviceSync1")]
impl DeviceSyncControl {
    fn snapshot(&self) -> Vec<DeviceSyncRow> {
        read_agent_device_sync_state(&self.state)
            .devices
            .into_iter()
            .map(|device| {
                (
                    device.name,
                    device.connected,
                    device.available_bytes.is_some(),
                    device.available_bytes.unwrap_or_default(),
                    device.total_bytes.is_some(),
                    device.total_bytes.unwrap_or_default(),
                    u64::try_from(device.managed_tracks).unwrap_or(u64::MAX),
                    u64::try_from(device.selected_tracks).unwrap_or(u64::MAX),
                    u64::try_from(device.tracks_to_copy).unwrap_or(u64::MAX),
                    u64::try_from(device.tracks_to_remove).unwrap_or(u64::MAX),
                    device.bytes_to_copy,
                    phase_name(&device.phase).to_owned(),
                    device.bytes_done,
                    device.bytes_total,
                    device.bytes_per_second,
                    device.current_track,
                )
            })
            .collect()
    }

    fn configure_playlist(
        &self,
        device_name: &str,
        playlist_name: &str,
        remove_unselected: bool,
        bitrate_kbps: u32,
    ) -> zbus::fdo::Result<()> {
        self.dispatch(AgentDeviceSyncCommand::ConfigurePlaylist {
            device_name: device_name.to_owned(),
            playlist_name: playlist_name.to_owned(),
            remove_unselected,
            bitrate_kbps,
        })
    }

    fn start(&self, device_name: &str) -> zbus::fdo::Result<()> {
        self.dispatch(AgentDeviceSyncCommand::Start {
            device_name: device_name.to_owned(),
        })
    }

    fn cancel(&self, device_name: &str) -> zbus::fdo::Result<()> {
        self.dispatch(AgentDeviceSyncCommand::Cancel {
            device_name: device_name.to_owned(),
        })
    }
}

fn phase_name(phase: &AgentDeviceSyncPhase) -> &'static str {
    match phase {
        AgentDeviceSyncPhase::Idle => "idle",
        AgentDeviceSyncPhase::ComputingDelta => "computing_delta",
        AgentDeviceSyncPhase::Removing => "removing",
        AgentDeviceSyncPhase::Transcoding => "transcoding",
        AgentDeviceSyncPhase::Copying => "copying",
        AgentDeviceSyncPhase::WritingPlaylists => "writing_playlists",
        AgentDeviceSyncPhase::Finishing => "finishing",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use reprise_core::agent_device_sync::{AgentDeviceSyncDevice, AgentDeviceSyncState};

    use super::*;

    #[test]
    fn snapshot_is_path_free_and_commands_keep_device_names() {
        let state = Arc::new(Mutex::new(AgentDeviceSyncState {
            devices: vec![AgentDeviceSyncDevice {
                name: "Pixel".into(),
                connected: true,
                available_bytes: Some(80),
                total_bytes: Some(100),
                selected_tracks: 200,
                bytes_per_second: 12,
                phase: AgentDeviceSyncPhase::Copying,
                ..AgentDeviceSyncDevice::default()
            }],
        }));
        let (sender, receiver) = async_channel::unbounded();
        let control = DeviceSyncControl::new(sender, state);

        let rows = control.snapshot();
        assert_eq!(rows[0].0, "Pixel");
        assert_eq!(rows[0].3, 80);
        assert_eq!(rows[0].7, 200);
        assert_eq!(rows[0].14, 12);

        let responder = std::thread::spawn(move || {
            let request = receiver.recv_blocking().unwrap();
            assert_eq!(
                request.command,
                AgentDeviceSyncCommand::ConfigurePlaylist {
                    device_name: "Pixel".into(),
                    playlist_name: "Lorna Shore & Similar".into(),
                    remove_unselected: true,
                    bitrate_kbps: 256,
                }
            );
            request.reply.send(Ok(())).unwrap();
        });
        control
            .configure_playlist("Pixel", "Lorna Shore & Similar", true, 256)
            .unwrap();
        responder.join().unwrap();
    }

    #[test]
    fn rejected_ui_request_is_returned_to_the_dbus_caller() {
        let state = Arc::new(Mutex::new(AgentDeviceSyncState::default()));
        let (sender, receiver) = async_channel::unbounded();
        let control = DeviceSyncControl::new(sender, state);
        let responder = std::thread::spawn(move || {
            let request = receiver.recv_blocking().unwrap();
            assert_eq!(
                request.command,
                AgentDeviceSyncCommand::Start {
                    device_name: "Missing".into(),
                }
            );
            request
                .reply
                .send(Err("device 'Missing' is absent".into()))
                .unwrap();
        });

        let error = control.start("Missing").unwrap_err();
        assert!(error.to_string().contains("device 'Missing' is absent"));
        responder.join().unwrap();
    }
}
