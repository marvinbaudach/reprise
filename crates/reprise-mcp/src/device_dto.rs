//! Path-free MCP data types for live Android device synchronization.

use rmcp::schemars;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, schemars::JsonSchema)]
pub struct GetDeviceSyncStateParams {}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct DeviceSyncParams {
    /// One of: configure_playlist, start, cancel.
    pub action: String,
    /// Exact display name from music_get_device_sync_state.
    pub device_name: String,
    /// Required only for configure_playlist.
    #[serde(default)]
    pub playlist_name: Option<String>,
    /// Whether managed tracks outside the selection should be removed.
    #[serde(default)]
    pub remove_unselected: Option<bool>,
    /// Opus conversion bitrate in kbit/s. Supported: 0, 64, 96, 128, 160, 192, 256.
    #[serde(default)]
    pub bitrate_kbps: Option<u32>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeviceSyncStateDto {
    pub devices: Vec<DeviceSyncDeviceDto>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeviceSyncDeviceDto {
    pub name: String,
    pub connected: bool,
    pub available_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    pub managed_tracks: u64,
    pub selected_tracks: u64,
    pub tracks_to_copy: u64,
    pub tracks_to_remove: u64,
    pub bytes_to_copy: u64,
    pub phase: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub bytes_per_second: u64,
    pub current_track: String,
}
