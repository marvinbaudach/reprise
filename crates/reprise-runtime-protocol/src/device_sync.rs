//! Device-synchronization snapshots and commands.
//!
//! These types replace the duplicated positional tuples described in the
//! crate documentation. The field names below are the wire contract; see
//! `tests/schema.rs`, which pins them against a fixture.

use serde::{Deserialize, Serialize};
use zvariant::{DeserializeDict, SerializeDict, Type};

/// One connected or remembered device and everything a client needs to
/// render or drive it.
#[derive(Debug, Clone, PartialEq, Eq, Default, SerializeDict, DeserializeDict, Type)]
#[zvariant(signature = "a{sv}")]
pub struct DeviceSnapshot {
    /// The device's display name; also its address in every command.
    pub name: String,
    pub connected: bool,
    /// Transfer profile as its stored value: `opus_160`, `mp3_256`, `original`.
    pub profile: String,
    pub managed_tracks: u64,
    pub unique_track_count: u64,
    pub target_bytes: u64,
    pub sources: Vec<DeviceSourceSnapshot>,
    pub changes: DeviceChangeCounts,
    pub storage: DeviceStorageSnapshot,
    /// Short diagnostic kinds, never prose and never a path — for example
    /// `no_playlists_selected` or `missing_playlist:playlist:12`.
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub controls: DeviceControls,
    /// The run's phase as a short kind: `idle`, `inspecting`, `transcoding`,
    /// `copying`, `writing_playlists`, `removing`, `verifying`, `failed`.
    pub phase: String,
    pub progress: DeviceProgress,
    /// Display title of the track currently being transferred; empty when
    /// nothing is in flight.
    pub current_track: String,
    /// Last verified synchronization as Unix UTC seconds. Absent when the
    /// device has never completed one.
    pub last_synced_at: Option<i64>,
    /// The three named sync targets and what the next run would do to each
    /// (`MTP-37`). Empty for a device whose targets could not be read.
    pub categories: Vec<DeviceCategorySnapshot>,
}

/// One selectable synchronization source (a playlist or a smart playlist).
#[derive(Debug, Clone, PartialEq, Eq, Default, SerializeDict, DeserializeDict, Type)]
#[zvariant(signature = "a{sv}")]
pub struct DeviceSourceSnapshot {
    /// `playlist` or `smart`.
    pub kind: String,
    pub id: i64,
    /// Absent when the source no longer resolves — the client renders the
    /// missing case rather than an empty name.
    pub name: Option<String>,
    pub selected: bool,
    pub available: bool,
    pub entry_count: u64,
    pub unique_track_count: u64,
    pub unavailable_count: u64,
    pub target_bytes: u64,
    pub last_synced_at: Option<i64>,
}

/// What the next run would change. Seven counters that used to be seven
/// consecutive `u64`s in a tuple.
#[derive(Debug, Clone, PartialEq, Eq, Default, SerializeDict, DeserializeDict, Type)]
#[zvariant(signature = "a{sv}")]
pub struct DeviceChangeCounts {
    pub additions: u64,
    pub replacements: u64,
    pub removals: u64,
    pub retained_unavailable: u64,
    pub playlist_writes: u64,
    pub playlist_removals: u64,
    pub transfer_bytes: u64,
}

/// Storage headroom before and after the planned run.
#[derive(Debug, Clone, PartialEq, Eq, Default, SerializeDict, DeserializeDict, Type)]
#[zvariant(signature = "a{sv}")]
pub struct DeviceStorageSnapshot {
    /// The storage volume's name, absent when the device exposes none.
    pub target_name: Option<String>,
    /// `fits`, `insufficient`, `capacity_unknown`, `inconsistent`, `blocked`.
    pub state: String,
    /// Present only for `insufficient`; the tuple encoded this as a
    /// `(bool, u64)` pair that both sides had to remember to check.
    pub shortfall_bytes: Option<u64>,
    pub transfer_bytes: u64,
    pub current: DeviceStorageComposition,
    /// Absent when the run cannot be projected.
    pub after_sync: Option<DeviceStorageComposition>,
    /// `writable`, `read_only`, `unknown`.
    pub access: String,
}

/// How a device's storage is used. Every byte count is optional exactly when
/// the device declines to report it; `knowledge` says why.
#[derive(Debug, Clone, PartialEq, Eq, Default, SerializeDict, DeserializeDict, Type)]
#[zvariant(signature = "a{sv}")]
pub struct DeviceStorageComposition {
    pub total_bytes: Option<u64>,
    pub reprise_music_bytes: u64,
    pub other_music_bytes: u64,
    pub other_used_bytes: Option<u64>,
    pub free_bytes: Option<u64>,
    /// `complete`, `capacity_unknown`, `inconsistent`.
    pub knowledge: String,
}

/// Which actions the runtime will currently accept for this device. A client
/// renders availability from these instead of re-deriving the rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, SerializeDict, DeserializeDict, Type)]
#[zvariant(signature = "a{sv}")]
pub struct DeviceControls {
    pub editable: bool,
    pub can_start: bool,
    pub can_cancel: bool,
    pub can_eject: bool,
}

/// Byte-level progress of a running transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, SerializeDict, DeserializeDict, Type)]
#[zvariant(signature = "a{sv}")]
pub struct DeviceProgress {
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub bytes_per_second: u64,
}

/// One named sync target's per-device state plus its `MTP-22` diff reading
/// (`MTP-38`, `MTP-37`).
///
/// This began life as a separate `CategorySnapshot` D-Bus method carrying a
/// 13-wide positional tuple, because zvariant only generates tuple `Type` and
/// `Serialize` impls up to a fixed arity and [`DeviceSnapshot`] was already at
/// 16. Now that these types are named dicts rather than tuples that ceiling is
/// gone, so the categories travel inside the snapshot where they belong and
/// the extra method is gone with them.
#[derive(Debug, Clone, PartialEq, Eq, Default, SerializeDict, DeserializeDict, Type)]
#[zvariant(signature = "a{sv}")]
pub struct DeviceCategorySnapshot {
    /// `playlists`, `youtube_audio` or `podcast_episodes`.
    pub kind: String,
    /// The target's folder on the device, as the folder browser left it.
    pub target_path: String,
    pub target_enabled: bool,
    pub size_on_device_bytes: u64,
    /// Absent means no cap, which is what a cap of zero means everywhere in
    /// Reprise (`MTP-36`, decided 2026-07-29).
    pub cap_bytes: Option<u64>,
    /// `diff`, `source_off` or `unavailable_kept_on_phone` — three states
    /// that `MTP-22` requires be distinguishable rather than flattened into
    /// one zero.
    pub reading_kind: String,
    pub files_to_copy: u64,
    pub bytes_to_copy: u64,
    pub files_to_remove: u64,
    pub bytes_freed: u64,
    /// Wanted but not yet downloaded (`MTP-40`) — counted apart from
    /// `files_to_copy` so the balance never promises a transfer that cannot
    /// happen yet.
    pub files_waiting_for_download: u64,
    pub playlists_rewritten: u64,
}

/// One source selection in a `Configure` command.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
pub struct DeviceSourceSelection {
    /// `playlist` or `smart`.
    pub kind: String,
    pub id: i64,
}
