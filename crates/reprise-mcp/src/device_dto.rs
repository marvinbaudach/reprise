//! Path-free MCP data types for live Android device synchronization.

use rmcp::schemars;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, schemars::JsonSchema)]
pub struct GetDeviceSyncStateParams {}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeviceSyncParams {
    /// One of: configure, start, cancel, eject.
    pub action: String,
    /// Exact display name from music_get_device_sync_state.
    pub device_name: String,
    /// Exact path-free source identities from music_get_device_sync_state.
    #[serde(default)]
    pub sources: Option<Vec<DeviceSyncSourceParam>>,
    /// Transfer profile: opus_160, mp3_256 or original. Defaults to opus_160.
    #[serde(default)]
    pub profile: Option<String>,
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
    /// Last verified device synchronization as Unix UTC seconds.
    pub last_synced_at: Option<i64>,
    pub profile: String,
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
    /// Block H (MCP parity, `MTP-38`/`MTP-22`): the three named sync
    /// targets, each with its target folder, cap and category diff reading.
    pub categories: Vec<DeviceSyncCategoryDto>,
    /// `MTP-22`'s aggregate balance across every category currently reading
    /// a computed diff — the same `aggregate_balance` projection the device
    /// page's sidebar card tooltip reads, computed here from `categories`
    /// rather than carried separately over the wire.
    pub balance: DeviceSyncBalanceDto,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeviceSyncCategoryDto {
    /// One of: `playlists`, `youtube_audio`, `podcast_episodes`.
    pub kind: &'static str,
    pub target_path: String,
    /// `SyncTarget::enabled` — whether this device's slot for the category
    /// is active at all (`MTP-38`), independent of the global rule.
    pub target_enabled: bool,
    pub size_on_device_bytes: u64,
    pub cap_bytes: Option<u64>,
    /// One of: `diff`, `source_off`, `unavailable_kept_on_phone`. `diff`
    /// carries the six count/byte fields below; the other two states leave
    /// them at zero on purpose — `source_off` means nothing was evaluated,
    /// not "evaluated to zero changes", and must never be mistaken for a
    /// genuine zero-change diff (`MTP-22`).
    pub reading: &'static str,
    pub files_to_copy: u64,
    pub bytes_to_copy: u64,
    pub files_to_remove: u64,
    pub bytes_freed: u64,
    pub files_waiting_for_download: u64,
    pub playlists_rewritten: u64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct DeviceSyncBalanceDto {
    pub files_to_copy: u64,
    pub bytes_to_copy: u64,
    pub files_to_remove: u64,
    pub bytes_freed: u64,
    pub files_waiting_for_download: u64,
    pub playlists_rewritten: u64,
    /// Same rule as `CategoryDiff::has_work`/`SyncBalance::has_work`: file
    /// counts decide, bytes never do — a deletions-only sync with 0 bytes
    /// moved must still read as work pending (design 7c).
    pub has_work: bool,
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
    /// Last verified synchronization of this playlist as Unix UTC seconds.
    pub last_synced_at: Option<i64>,
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
    pub access: String,
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
    pub can_eject: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeviceSyncProgressDto {
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub bytes_per_second: u64,
}
