//! Reprise-specific local D-Bus surface for live device synchronization.

use zbus::interface;

use reprise_core::agent_device_sync::{
    agent_device_sync_request, read_agent_device_sync_state, AgentDeviceSyncBlocker,
    AgentDeviceSyncCategoryRow, AgentDeviceSyncChanges, AgentDeviceSyncCommand,
    AgentDeviceSyncControls, AgentDeviceSyncDevice, AgentDeviceSyncPhase, AgentDeviceSyncPlaylist,
    AgentDeviceSyncRequest, AgentDeviceSyncStorage, AgentDeviceSyncStorageAccess,
    AgentDeviceSyncStorageComposition, AgentDeviceSyncStorageKnowledge,
    AgentDeviceSyncStorageState, AgentDeviceSyncWarning, SharedAgentDeviceSyncState,
};
use reprise_core::device_sync::{CategoryDiff, CategoryReading, SelectionSource, TransferProfile};

pub(super) type DeviceSyncSourceSelection = (String, i64);
pub(super) type DeviceSyncSourceRow = (
    String,
    i64,
    bool,
    String,
    bool,
    bool,
    u64,
    u64,
    u64,
    u64,
    bool,
    i64,
);
pub(super) type DeviceSyncChangesRow = (u64, u64, u64, u64, u64, u64, u64);
pub(super) type DeviceSyncStorageCompositionRow =
    (bool, u64, u64, u64, bool, u64, bool, u64, String);
pub(super) type DeviceSyncStorageRow = (
    bool,
    String,
    String,
    bool,
    u64,
    u64,
    DeviceSyncStorageCompositionRow,
    bool,
    DeviceSyncStorageCompositionRow,
    String,
);
/// Block H (MCP parity): one named sync target's per-device state plus its
/// `MTP-22` diff reading. Kept as its own D-Bus method (`CategorySnapshot`)
/// rather than a 17th element of the already 16-wide [`DeviceSyncRow`] —
/// zvariant's tuple `Type`/`Serialize` impls are only generated up to a
/// fixed arity, and this keeps the existing, well-tested `Snapshot` wire
/// shape untouched.
///
/// Fields: kind, target_path, target_enabled, size_on_device_bytes,
/// has_cap, cap_bytes, reading_kind ("diff" | "source_off" |
/// "unavailable_kept_on_phone"), files_to_copy, bytes_to_copy,
/// files_to_remove, bytes_freed, files_waiting_for_download,
/// playlists_rewritten.
pub(super) type DeviceSyncCategoryRow = (
    String,
    String,
    bool,
    u64,
    bool,
    u64,
    String,
    u64,
    u64,
    u64,
    u64,
    u64,
    u64,
);
pub(super) type DeviceSyncCategoryDeviceRow = (String, Vec<DeviceSyncCategoryRow>);

pub(super) type DeviceSyncControlsRow = (bool, bool, bool, bool);
pub(super) type DeviceSyncProgressRow = (u64, u64, u64);
pub(super) type DeviceSyncTimestampRow = (bool, i64);
pub(super) type DeviceSyncRow = (
    String,
    bool,
    String,
    u64,
    u64,
    u64,
    Vec<DeviceSyncSourceRow>,
    DeviceSyncChangesRow,
    DeviceSyncStorageRow,
    Vec<String>,
    Vec<String>,
    DeviceSyncControlsRow,
    String,
    DeviceSyncProgressRow,
    String,
    DeviceSyncTimestampRow,
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
            .map(device_row)
            .collect()
    }

    /// Block H (MCP parity, `MTP-18`/`MTP-22`): the three named sync
    /// targets and their category diff reading per device, keyed by device
    /// name — the same identity `Configure`/`Start`/`Cancel`/`Eject`
    /// already address a device by.
    fn category_snapshot(&self) -> Vec<DeviceSyncCategoryDeviceRow> {
        read_agent_device_sync_state(&self.state)
            .devices
            .into_iter()
            .map(|device| {
                (
                    device.name,
                    device.categories.into_iter().map(category_row).collect(),
                )
            })
            .collect()
    }

    fn configure(
        &self,
        device_name: &str,
        sources: Vec<DeviceSyncSourceSelection>,
        profile: &str,
    ) -> zbus::fdo::Result<()> {
        let sources = sources
            .into_iter()
            .map(decode_source)
            .collect::<zbus::fdo::Result<Vec<_>>>()?;
        let profile = TransferProfile::from_storage_value(profile).ok_or_else(|| {
            zbus::fdo::Error::InvalidArgs(format!(
                "unknown transfer profile '{profile}'; expected opus_160, mp3_256 or original"
            ))
        })?;
        self.dispatch(AgentDeviceSyncCommand::Configure {
            device_name: device_name.to_owned(),
            sources,
            profile,
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

    fn eject(&self, device_name: &str) -> zbus::fdo::Result<()> {
        self.dispatch(AgentDeviceSyncCommand::Eject {
            device_name: device_name.to_owned(),
        })
    }
}

fn device_row(device: AgentDeviceSyncDevice) -> DeviceSyncRow {
    (
        device.name,
        device.connected,
        device.profile.storage_value().to_owned(),
        count(device.managed_tracks),
        count(device.unique_track_count),
        device.target_bytes,
        device.playlists.into_iter().map(source_row).collect(),
        changes_row(&device.changes),
        storage_row(&device.storage),
        device.blockers.into_iter().map(blocker_name).collect(),
        device.warnings.into_iter().map(warning_name).collect(),
        controls_row(device.controls),
        phase_name(&device.phase).to_owned(),
        (
            device.bytes_done,
            device.bytes_total,
            device.bytes_per_second,
        ),
        device.current_track,
        optional_timestamp(device.last_synced_at),
    )
}

fn target_kind_name(kind: reprise_core::device_sync::SyncTargetKind) -> &'static str {
    use reprise_core::device_sync::SyncTargetKind;
    match kind {
        SyncTargetKind::Playlists => "playlists",
        SyncTargetKind::YoutubeAudio => "youtube_audio",
        SyncTargetKind::PodcastEpisodes => "podcast_episodes",
    }
}

fn category_row(category: AgentDeviceSyncCategoryRow) -> DeviceSyncCategoryRow {
    let (reading_kind, diff) = match category.reading {
        CategoryReading::Diff(diff) => ("diff", diff),
        CategoryReading::SourceOff => ("source_off", CategoryDiff::default()),
        CategoryReading::UnavailableKeptOnPhone => {
            ("unavailable_kept_on_phone", CategoryDiff::default())
        }
    };
    (
        target_kind_name(category.kind).to_owned(),
        category.target_path,
        category.target_enabled,
        category.size_on_device_bytes,
        category.cap_bytes.is_some(),
        category.cap_bytes.unwrap_or_default(),
        reading_kind.to_owned(),
        count(diff.files_to_copy),
        diff.bytes_to_copy,
        count(diff.files_to_remove),
        diff.bytes_freed,
        count(diff.files_waiting_for_download),
        count(diff.playlists_rewritten),
    )
}

fn source_row(source: AgentDeviceSyncPlaylist) -> DeviceSyncSourceRow {
    let (kind, id) = source_parts(&source.source);
    (
        kind.to_owned(),
        id,
        source.name.is_some(),
        source.name.unwrap_or_default(),
        source.selected,
        source.available,
        count(source.entry_count),
        count(source.unique_track_count),
        count(source.unavailable_count),
        source.target_bytes,
        source.last_synced_at.is_some(),
        source.last_synced_at.unwrap_or_default(),
    )
}

fn changes_row(changes: &AgentDeviceSyncChanges) -> DeviceSyncChangesRow {
    (
        count(changes.additions),
        count(changes.replacements),
        count(changes.removals),
        count(changes.retained_unavailable),
        count(changes.playlist_writes),
        count(changes.playlist_removals),
        changes.transfer_bytes,
    )
}

fn storage_row(storage: &AgentDeviceSyncStorage) -> DeviceSyncStorageRow {
    let (state, has_shortfall, shortfall) = storage_state_name(storage.state);
    let empty = AgentDeviceSyncStorageComposition::default();
    let after_sync = storage.after_sync.as_ref();
    (
        storage.target_name.is_some(),
        storage.target_name.clone().unwrap_or_default(),
        state.to_owned(),
        has_shortfall,
        shortfall,
        storage.transfer_bytes,
        storage_composition_row(&storage.current),
        after_sync.is_some(),
        storage_composition_row(after_sync.unwrap_or(&empty)),
        storage_access_name(storage.access).to_owned(),
    )
}

fn optional_timestamp(timestamp: Option<i64>) -> DeviceSyncTimestampRow {
    (timestamp.is_some(), timestamp.unwrap_or_default())
}

fn storage_access_name(access: AgentDeviceSyncStorageAccess) -> &'static str {
    match access {
        AgentDeviceSyncStorageAccess::Writable => "writable",
        AgentDeviceSyncStorageAccess::ReadOnly => "read_only",
        AgentDeviceSyncStorageAccess::Unknown => "unknown",
    }
}

fn storage_composition_row(
    composition: &AgentDeviceSyncStorageComposition,
) -> DeviceSyncStorageCompositionRow {
    (
        composition.total_bytes.is_some(),
        composition.total_bytes.unwrap_or_default(),
        composition.reprise_music_bytes,
        composition.other_music_bytes,
        composition.other_used_bytes.is_some(),
        composition.other_used_bytes.unwrap_or_default(),
        composition.free_bytes.is_some(),
        composition.free_bytes.unwrap_or_default(),
        match composition.knowledge {
            AgentDeviceSyncStorageKnowledge::Complete => "complete",
            AgentDeviceSyncStorageKnowledge::CapacityUnknown => "capacity_unknown",
            AgentDeviceSyncStorageKnowledge::Inconsistent => "inconsistent",
        }
        .to_owned(),
    )
}

fn controls_row(controls: AgentDeviceSyncControls) -> DeviceSyncControlsRow {
    (
        controls.editable,
        controls.can_start,
        controls.can_cancel,
        controls.can_eject,
    )
}

fn storage_state_name(state: AgentDeviceSyncStorageState) -> (&'static str, bool, u64) {
    match state {
        AgentDeviceSyncStorageState::Fits => ("fits", false, 0),
        AgentDeviceSyncStorageState::Insufficient { shortfall_bytes } => {
            ("insufficient", true, shortfall_bytes)
        }
        AgentDeviceSyncStorageState::CapacityUnknown => ("capacity_unknown", false, 0),
        AgentDeviceSyncStorageState::Inconsistent => ("inconsistent", false, 0),
        AgentDeviceSyncStorageState::Blocked => ("blocked", false, 0),
    }
}

fn blocker_name(blocker: AgentDeviceSyncBlocker) -> String {
    match blocker {
        AgentDeviceSyncBlocker::NoPlaylistsSelected => "no_playlists_selected".into(),
        AgentDeviceSyncBlocker::MissingPlaylist(source) => {
            format!("missing_playlist:{}", source_token(&source))
        }
        AgentDeviceSyncBlocker::DuplicatePlaylist(source) => {
            format!("duplicate_playlist:{}", source_token(&source))
        }
    }
}

fn warning_name(warning: AgentDeviceSyncWarning) -> String {
    match warning {
        AgentDeviceSyncWarning::UnavailableNotOnDevice => "unavailable_not_on_device".into(),
        AgentDeviceSyncWarning::UnsafeManagedItem => "unsafe_managed_item".into(),
    }
}

fn source_parts(source: &SelectionSource) -> (&'static str, i64) {
    match source {
        SelectionSource::Playlist(id) => ("playlist", *id),
        SelectionSource::Smart(id) => ("smart", *id),
    }
}

fn source_token(source: &SelectionSource) -> String {
    let (kind, id) = source_parts(source);
    format!("{kind}:{id}")
}

fn decode_source((kind, id): DeviceSyncSourceSelection) -> zbus::fdo::Result<SelectionSource> {
    if id <= 0 {
        return Err(zbus::fdo::Error::InvalidArgs(
            "playlist source ids must be positive".into(),
        ));
    }
    match kind.as_str() {
        "playlist" => Ok(SelectionSource::Playlist(id)),
        "smart" => Ok(SelectionSource::Smart(id)),
        _ => Err(zbus::fdo::Error::InvalidArgs(format!(
            "unknown playlist source kind '{kind}'"
        ))),
    }
}

fn count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
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
    fn snapshot_carries_the_compact_mirror_page_and_commands_use_source_identity() {
        let state = Arc::new(Mutex::new(AgentDeviceSyncState {
            devices: vec![AgentDeviceSyncDevice {
                name: "Pixel".into(),
                connected: true,
                last_synced_at: Some(1_721_234_890),
                profile: TransferProfile::Original,
                unique_track_count: 200,
                target_bytes: 80,
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
                changes: AgentDeviceSyncChanges {
                    additions: 125,
                    transfer_bytes: 60,
                    ..AgentDeviceSyncChanges::default()
                },
                storage: AgentDeviceSyncStorage {
                    state: AgentDeviceSyncStorageState::Fits,
                    current: AgentDeviceSyncStorageComposition {
                        total_bytes: Some(100),
                        free_bytes: Some(80),
                        knowledge: AgentDeviceSyncStorageKnowledge::Complete,
                        ..AgentDeviceSyncStorageComposition::default()
                    },
                    ..AgentDeviceSyncStorage::default()
                },
                controls: AgentDeviceSyncControls {
                    editable: false,
                    can_start: false,
                    can_cancel: true,
                    can_eject: false,
                },
                bytes_per_second: 12,
                phase: AgentDeviceSyncPhase::Copying,
                ..AgentDeviceSyncDevice::default()
            }],
        }));
        let (sender, receiver) = async_channel::unbounded();
        let control = DeviceSyncControl::new(sender, state);

        let rows = control.snapshot();
        assert_eq!(rows[0].0, "Pixel");
        assert_eq!(rows[0].2, "original");
        assert_eq!(rows[0].4, 200);
        assert_eq!(rows[0].6[0].0, "smart");
        assert_eq!(rows[0].6[0].1, 7);
        assert_eq!(rows[0].6[0].6, 220);
        assert!(rows[0].6[0].10);
        assert_eq!(rows[0].6[0].11, 1_721_234_567);
        assert_eq!(rows[0].7 .0, 125);
        assert_eq!(rows[0].8 .6 .7, 80);
        assert!(rows[0].11 .2);
        assert!(!rows[0].11 .3);
        assert_eq!(rows[0].13 .2, 12);
        assert_eq!(rows[0].15, (true, 1_721_234_890));

        let responder = std::thread::spawn(move || {
            let request = receiver.recv_blocking().unwrap();
            assert_eq!(
                request.command,
                AgentDeviceSyncCommand::Configure {
                    device_name: "Pixel".into(),
                    sources: vec![SelectionSource::Playlist(3), SelectionSource::Smart(7),],
                    profile: TransferProfile::Opus160,
                }
            );
            request.reply.send(Ok(())).unwrap();
        });
        control
            .configure(
                "Pixel",
                vec![("playlist".into(), 3), ("smart".into(), 7)],
                "opus_160",
            )
            .unwrap();
        responder.join().unwrap();
    }

    /// Block H (MCP parity): `category_snapshot` must actually distinguish
    /// its three `MTP-22` reading states, not just carry a label field that
    /// stays green if the distinction were lost.
    #[test]
    fn category_snapshot_distinguishes_diff_source_off_and_unavailable_readings() {
        use reprise_core::device_sync::{CategoryDiff, CategoryReading, SyncTargetKind};

        let category = |kind, reading| AgentDeviceSyncCategoryRow {
            kind,
            target_path: "/Music/Reprise-YouTube".into(),
            target_enabled: true,
            size_on_device_bytes: 42,
            cap_bytes: Some(8 * 1024 * 1024 * 1024),
            reading,
        };
        let state = Arc::new(Mutex::new(AgentDeviceSyncState {
            devices: vec![AgentDeviceSyncDevice {
                name: "Pixel".into(),
                categories: vec![
                    category(
                        SyncTargetKind::YoutubeAudio,
                        CategoryReading::Diff(CategoryDiff {
                            files_to_copy: 3,
                            bytes_to_copy: 900,
                            files_to_remove: 1,
                            bytes_freed: 50,
                            files_waiting_for_download: 2,
                            playlists_rewritten: 0,
                        }),
                    ),
                    category(SyncTargetKind::PodcastEpisodes, CategoryReading::SourceOff),
                    category(
                        SyncTargetKind::Playlists,
                        CategoryReading::UnavailableKeptOnPhone,
                    ),
                ],
                ..AgentDeviceSyncDevice::default()
            }],
        }));
        let (sender, _receiver) = async_channel::unbounded();
        let control = DeviceSyncControl::new(sender, state);

        let snapshot = control.category_snapshot();

        assert_eq!(snapshot.len(), 1);
        let (name, categories) = &snapshot[0];
        assert_eq!(name, "Pixel");
        assert_eq!(categories[0].0, "youtube_audio");
        assert_eq!(categories[0].6, "diff");
        assert_eq!(categories[0].7, 3, "files_to_copy survives the wire");
        assert_eq!(categories[0].10, 50, "bytes_freed survives the wire");
        assert_eq!(categories[0].11, 2, "files_waiting_for_download survives");
        assert!(categories[0].4, "has_cap");
        assert_eq!(categories[0].5, 8 * 1024 * 1024 * 1024);

        assert_eq!(categories[1].0, "podcast_episodes");
        assert_eq!(
            categories[1].6, "source_off",
            "source_off must not read as a zero diff"
        );
        assert_eq!(
            categories[1].7, 0,
            "source_off carries no diff figures, unlike a genuine zero-change diff"
        );

        assert_eq!(categories[2].0, "playlists");
        assert_eq!(categories[2].6, "unavailable_kept_on_phone");
        assert_ne!(
            categories[1].6, categories[2].6,
            "source_off and unavailable_kept_on_phone are distinct states"
        );
    }

    #[test]
    fn configure_rejects_invalid_source_identity_before_dispatch() {
        assert!(decode_source(("playlist".into(), 0)).is_err());
        assert!(decode_source(("unknown".into(), 1)).is_err());
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

    #[test]
    fn eject_dispatches_the_path_free_device_identity() {
        let state = Arc::new(Mutex::new(AgentDeviceSyncState::default()));
        let (sender, receiver) = async_channel::unbounded();
        let control = DeviceSyncControl::new(sender, state);
        let responder = std::thread::spawn(move || {
            let request = receiver.recv_blocking().unwrap();
            assert_eq!(
                request.command,
                AgentDeviceSyncCommand::Eject {
                    device_name: "Pixel".into(),
                }
            );
            request.reply.send(Ok(())).unwrap();
        });

        control.eject("Pixel").unwrap();
        responder.join().unwrap();
    }
}
