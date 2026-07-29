//! Path-free live device-synchronization contract shared by frontends.

use std::sync::{mpsc, Arc, Mutex};

use crate::device_sync::{CategoryReading, SelectionSource, SyncTargetKind, TransferProfile};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentDeviceSyncState {
    pub devices: Vec<AgentDeviceSyncDevice>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentDeviceSyncDevice {
    pub name: String,
    pub connected: bool,
    pub last_synced_at: Option<i64>,
    pub managed_tracks: usize,
    pub profile: TransferProfile,
    pub playlists: Vec<AgentDeviceSyncPlaylist>,
    pub unique_track_count: usize,
    pub target_bytes: u64,
    pub changes: AgentDeviceSyncChanges,
    pub storage: AgentDeviceSyncStorage,
    pub blockers: Vec<AgentDeviceSyncBlocker>,
    pub warnings: Vec<AgentDeviceSyncWarning>,
    pub controls: AgentDeviceSyncControls,
    pub phase: AgentDeviceSyncPhase,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub bytes_per_second: u64,
    pub current_track: String,
    /// Block H (MCP parity): the three named sync targets (`MTP-38`) plus
    /// their `MTP-22` category reading, in `SyncTargetKind::ALL` order.
    /// Reuses `reprise_core::device_sync`'s own `SyncTargetKind` and
    /// `CategoryReading` rather than re-deriving a parallel shape — this is
    /// exactly the same data `DeviceView::content_rows`/`category_readings`
    /// already carry for the GTK device page.
    pub categories: Vec<AgentDeviceSyncCategoryRow>,
}

/// One of the three named sync targets (`MTP-38`) as seen by an agent: its
/// per-device folder, activation, on-device size, cap, and `MTP-22` diff
/// reading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDeviceSyncCategoryRow {
    pub kind: SyncTargetKind,
    pub target_path: String,
    pub target_enabled: bool,
    pub size_on_device_bytes: u64,
    pub cap_bytes: Option<u64>,
    pub reading: CategoryReading,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentDeviceSyncPlaylist {
    pub source: SelectionSource,
    pub name: Option<String>,
    pub selected: bool,
    pub available: bool,
    pub entry_count: usize,
    pub unique_track_count: usize,
    pub unavailable_count: usize,
    pub target_bytes: u64,
    pub last_synced_at: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentDeviceSyncChanges {
    pub additions: usize,
    pub replacements: usize,
    pub removals: usize,
    pub retained_unavailable: usize,
    pub playlist_writes: usize,
    pub playlist_removals: usize,
    pub transfer_bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentDeviceSyncStorage {
    pub target_name: Option<String>,
    pub access: AgentDeviceSyncStorageAccess,
    pub state: AgentDeviceSyncStorageState,
    pub transfer_bytes: u64,
    pub current: AgentDeviceSyncStorageComposition,
    pub after_sync: Option<AgentDeviceSyncStorageComposition>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AgentDeviceSyncStorageAccess {
    Writable,
    ReadOnly,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AgentDeviceSyncStorageState {
    Fits,
    Insufficient {
        shortfall_bytes: u64,
    },
    #[default]
    CapacityUnknown,
    Inconsistent,
    Blocked,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentDeviceSyncStorageComposition {
    pub total_bytes: Option<u64>,
    pub reprise_music_bytes: u64,
    pub other_music_bytes: u64,
    pub other_used_bytes: Option<u64>,
    pub free_bytes: Option<u64>,
    pub knowledge: AgentDeviceSyncStorageKnowledge,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AgentDeviceSyncStorageKnowledge {
    Complete,
    #[default]
    CapacityUnknown,
    Inconsistent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentDeviceSyncBlocker {
    NoPlaylistsSelected,
    MissingPlaylist(SelectionSource),
    DuplicatePlaylist(SelectionSource),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentDeviceSyncWarning {
    UnavailableNotOnDevice,
    UnsafeManagedItem,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AgentDeviceSyncControls {
    pub editable: bool,
    pub can_start: bool,
    pub can_cancel: bool,
    pub can_eject: bool,
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
    Configure {
        device_name: String,
        sources: Vec<SelectionSource>,
        profile: TransferProfile,
    },
    Start {
        device_name: String,
    },
    Cancel {
        device_name: String,
    },
    Eject {
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
    use crate::device_sync::SelectionSource;

    #[test]
    fn agent_snapshot_exposes_the_complete_path_free_playlist_mirror_state() {
        let state = Arc::new(Mutex::new(AgentDeviceSyncState {
            devices: vec![AgentDeviceSyncDevice {
                name: "Pixel".into(),
                connected: true,
                last_synced_at: Some(1_721_234_890),
                managed_tracks: 75,
                profile: TransferProfile::Original,
                playlists: vec![AgentDeviceSyncPlaylist {
                    source: SelectionSource::Smart(7),
                    name: Some("Heavy rotation".into()),
                    selected: true,
                    available: true,
                    entry_count: 220,
                    unique_track_count: 200,
                    unavailable_count: 2,
                    target_bytes: 80,
                    last_synced_at: Some(1_721_234_567),
                }],
                unique_track_count: 200,
                target_bytes: 80,
                changes: AgentDeviceSyncChanges {
                    additions: 120,
                    replacements: 5,
                    removals: 0,
                    retained_unavailable: 2,
                    playlist_writes: 1,
                    playlist_removals: 0,
                    transfer_bytes: 60,
                },
                storage: AgentDeviceSyncStorage {
                    target_name: Some("Internal storage".into()),
                    access: AgentDeviceSyncStorageAccess::Writable,
                    state: AgentDeviceSyncStorageState::Fits,
                    transfer_bytes: 60,
                    current: AgentDeviceSyncStorageComposition {
                        total_bytes: Some(100),
                        reprise_music_bytes: 20,
                        other_music_bytes: 10,
                        other_used_bytes: Some(30),
                        free_bytes: Some(40),
                        knowledge: AgentDeviceSyncStorageKnowledge::Complete,
                    },
                    after_sync: Some(AgentDeviceSyncStorageComposition {
                        total_bytes: Some(100),
                        reprise_music_bytes: 80,
                        other_music_bytes: 10,
                        other_used_bytes: Some(10),
                        free_bytes: Some(0),
                        knowledge: AgentDeviceSyncStorageKnowledge::Complete,
                    }),
                },
                blockers: Vec::new(),
                warnings: vec![AgentDeviceSyncWarning::UnavailableNotOnDevice],
                controls: AgentDeviceSyncControls {
                    editable: false,
                    can_start: false,
                    can_cancel: true,
                    can_eject: false,
                },
                phase: AgentDeviceSyncPhase::Copying,
                bytes_done: 20,
                bytes_total: 60,
                bytes_per_second: 10,
                current_track: "Sun//Eater — Lorna Shore".into(),
                categories: vec![AgentDeviceSyncCategoryRow {
                    kind: crate::device_sync::SyncTargetKind::YoutubeAudio,
                    target_path: "/Music/Reprise-YouTube".into(),
                    target_enabled: true,
                    size_on_device_bytes: 42,
                    cap_bytes: Some(8 * 1024 * 1024 * 1024),
                    reading: CategoryReading::Diff(crate::device_sync::CategoryDiff {
                        files_to_copy: 3,
                        bytes_to_copy: 900,
                        files_to_remove: 1,
                        bytes_freed: 50,
                        files_waiting_for_download: 2,
                        playlists_rewritten: 0,
                    }),
                }],
            }],
        }));

        let snapshot = read_agent_device_sync_state(&state);
        let device = &snapshot.devices[0];
        assert_eq!(device.profile, TransferProfile::Original);
        assert_eq!(device.last_synced_at, Some(1_721_234_890));
        assert_eq!(device.playlists[0].source, SelectionSource::Smart(7));
        assert_eq!(device.playlists[0].entry_count, 220);
        assert_eq!(device.playlists[0].last_synced_at, Some(1_721_234_567));
        assert_eq!(device.changes.replacements, 5);
        assert_eq!(
            device.storage.access,
            AgentDeviceSyncStorageAccess::Writable
        );
        assert_eq!(device.storage.current.free_bytes, Some(40));
        assert_eq!(
            device.storage.after_sync.as_ref().unwrap().free_bytes,
            Some(0)
        );
        assert_eq!(
            device.warnings,
            [AgentDeviceSyncWarning::UnavailableNotOnDevice]
        );
        assert!(device.controls.can_cancel);
        assert_eq!(device.bytes_per_second, 10);
        assert_eq!(device.current_track, "Sun//Eater — Lorna Shore");
        assert_eq!(device.categories[0].kind, SyncTargetKind::YoutubeAudio);
        assert_eq!(device.categories[0].cap_bytes, Some(8 * 1024 * 1024 * 1024));
        assert_eq!(
            device.categories[0].reading,
            CategoryReading::Diff(crate::device_sync::CategoryDiff {
                files_to_copy: 3,
                bytes_to_copy: 900,
                files_to_remove: 1,
                bytes_freed: 50,
                files_waiting_for_download: 2,
                playlists_rewritten: 0,
            })
        );
    }

    #[test]
    fn configure_command_uses_stable_manual_and_smart_playlist_identity() {
        assert_eq!(
            AgentDeviceSyncCommand::Configure {
                device_name: "Pixel".into(),
                sources: vec![SelectionSource::Playlist(3), SelectionSource::Smart(7),],
                profile: TransferProfile::Opus160,
            },
            AgentDeviceSyncCommand::Configure {
                device_name: "Pixel".into(),
                sources: vec![SelectionSource::Playlist(3), SelectionSource::Smart(7),],
                profile: TransferProfile::Opus160,
            }
        );
    }
}
