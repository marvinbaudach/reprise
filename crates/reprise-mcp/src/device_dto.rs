//! Path-free MCP data types for live Android device synchronization.

use rmcp::schemars;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, schemars::JsonSchema)]
pub struct GetDeviceSyncStateParams {}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeviceSyncParams {
    /// One of: configure, start, cancel.
    pub action: String,
    /// Exact display name from music_get_device_sync_state.
    pub device_name: String,
    /// Exact path-free source identities from music_get_device_sync_state.
    #[serde(default)]
    pub sources: Option<Vec<DeviceSyncSourceParam>>,
    /// MP3 CBR quality in kbit/s. Supported: 128, 192, 256, 320. Defaults to 256.
    #[serde(default)]
    pub quality_kbps: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeviceSyncSourceParam {
    /// Source kind from state: playlist or smart.
    pub kind: String,
    /// Positive source id from state.
    pub id: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeviceSyncStateDto {
    pub devices: Vec<DeviceSyncDeviceDto>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeviceSyncDeviceDto {
    pub name: String,
    pub connected: bool,
    pub quality_kbps: u32,
    pub managed_tracks: u64,
    pub unique_track_count: u64,
    pub target_bytes: u64,
    pub playlists: Vec<DeviceSyncPlaylistDto>,
    pub changes: DeviceSyncChangesDto,
    pub storage: DeviceSyncStorageDto,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub controls: DeviceSyncControlsDto,
    pub phase: String,
    pub progress: DeviceSyncProgressDto,
    pub current_track: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeviceSyncPlaylistDto {
    pub kind: String,
    pub id: i64,
    pub name: Option<String>,
    pub selected: bool,
    pub available: bool,
    pub entry_count: u64,
    pub unique_track_count: u64,
    pub unavailable_count: u64,
    pub target_bytes: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeviceSyncChangesDto {
    pub additions: u64,
    pub replacements: u64,
    pub removals: u64,
    pub retained_unavailable: u64,
    pub playlist_writes: u64,
    pub playlist_removals: u64,
    pub transfer_bytes: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeviceSyncStorageDto {
    pub target_name: Option<String>,
    pub state: String,
    pub shortfall_bytes: Option<u64>,
    pub transfer_bytes: u64,
    pub current: DeviceSyncStorageCompositionDto,
    pub after_sync: Option<DeviceSyncStorageCompositionDto>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeviceSyncStorageCompositionDto {
    pub total_bytes: Option<u64>,
    pub reprise_music_bytes: u64,
    pub other_music_bytes: u64,
    pub other_used_bytes: Option<u64>,
    pub free_bytes: Option<u64>,
    pub knowledge: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeviceSyncControlsDto {
    pub editable: bool,
    pub can_start: bool,
    pub can_cancel: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeviceSyncProgressDto {
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub bytes_per_second: u64,
}
