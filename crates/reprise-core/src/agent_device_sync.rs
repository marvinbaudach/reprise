//! Path-free live device-synchronization contract shared by frontends.

use std::sync::{mpsc, Arc, Mutex};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentDeviceSyncState {
    pub devices: Vec<AgentDeviceSyncDevice>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentDeviceSyncDevice {
    pub name: String,
    pub connected: bool,
    pub available_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    pub managed_tracks: usize,
    pub selected_tracks: usize,
    pub tracks_to_copy: usize,
    pub tracks_to_remove: usize,
    pub bytes_to_copy: u64,
    pub phase: AgentDeviceSyncPhase,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub bytes_per_second: u64,
    pub current_track: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum AgentDeviceSyncPhase {
    #[default]
    Idle,
    ComputingDelta,
    Removing,
    Transcoding,
    Copying,
    WritingPlaylists,
    Finishing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentDeviceSyncCommand {
    ConfigurePlaylist {
        device_name: String,
        playlist_name: String,
        remove_unselected: bool,
        bitrate_kbps: u32,
    },
    Start {
        device_name: String,
    },
    Cancel {
        device_name: String,
    },
}

pub type AgentDeviceSyncReply = Result<(), String>;

pub struct AgentDeviceSyncRequest {
    pub command: AgentDeviceSyncCommand,
    pub reply: mpsc::SyncSender<AgentDeviceSyncReply>,
}

pub fn agent_device_sync_request(
    command: AgentDeviceSyncCommand,
) -> (AgentDeviceSyncRequest, mpsc::Receiver<AgentDeviceSyncReply>) {
    let (reply, receiver) = mpsc::sync_channel(1);
    (AgentDeviceSyncRequest { command, reply }, receiver)
}

pub type SharedAgentDeviceSyncState = Arc<Mutex<AgentDeviceSyncState>>;

pub fn read_agent_device_sync_state(state: &SharedAgentDeviceSyncState) -> AgentDeviceSyncState {
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_snapshot_exposes_live_capacity_delta_and_transfer_rate() {
        let state = Arc::new(Mutex::new(AgentDeviceSyncState {
            devices: vec![AgentDeviceSyncDevice {
                name: "Pixel".into(),
                connected: true,
                available_bytes: Some(40),
                total_bytes: Some(100),
                managed_tracks: 75,
                selected_tracks: 200,
                tracks_to_copy: 125,
                tracks_to_remove: 0,
                bytes_to_copy: 60,
                phase: AgentDeviceSyncPhase::Copying,
                bytes_done: 20,
                bytes_total: 60,
                bytes_per_second: 10,
                current_track: "Sun//Eater — Lorna Shore".into(),
            }],
        }));

        let snapshot = read_agent_device_sync_state(&state);
        let device = &snapshot.devices[0];
        assert_eq!(device.available_bytes, Some(40));
        assert_eq!(device.total_bytes, Some(100));
        assert_eq!(device.selected_tracks, 200);
        assert_eq!(device.bytes_per_second, 10);
        assert_eq!(device.current_track, "Sun//Eater — Lorna Shore");
    }
}
