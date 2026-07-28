//! The moving part of a device synchronization run.
//!
//! [`crate::device_sync::DeviceSnapshot`] describes a device *page*: what
//! sits on the device, what the next run would change, how much room is
//! left. Almost all of it comes from inspecting the device and the library,
//! which happens once per configuration change. This type is the part that
//! moves on every step of a run and is therefore published as a delta.
//!
//! It is a separate type rather than a partially filled `DeviceSnapshot`
//! because a mostly-default `DeviceSnapshot` is indistinguishable from a real
//! one that happens to report zeros — a client would render "0 bytes free"
//! as fact. Splitting the facets makes the unknown half absent instead of
//! zero.

use zvariant::{DeserializeDict, SerializeDict, Type};

use crate::device_sync::DeviceProgress;

/// Live state of one device run.
#[derive(Debug, Clone, PartialEq, Eq, Default, SerializeDict, DeserializeDict, Type)]
#[zvariant(signature = "a{sv}")]
pub struct DeviceRunSnapshot {
    /// The device's display name — the same address every command uses.
    pub device: String,
    /// The phase, drawn from `DeviceSnapshot::phase`'s vocabulary so a client
    /// renders both snapshots with one mapping: `idle`, `inspecting`,
    /// `removing`, `transcoding`, `copying`, `writing_playlists`,
    /// `verifying`, `failed`.
    pub phase: String,
    pub progress: DeviceProgress,
    /// Display title of the track currently in flight; empty when none is.
    pub current_track: String,
    /// Tracks the run could not transfer, by library id. Never a path and
    /// never the underlying error text — a failed transfer is exactly the
    /// place where quoting the filesystem is tempting.
    pub failed_track_ids: Vec<i64>,
    /// How the run ended: `completed`, `cancelled`, `failed`. Absent while
    /// the run is still going, which is what distinguishes "running and
    /// currently at zero progress" from "finished".
    pub outcome: Option<String>,
}
