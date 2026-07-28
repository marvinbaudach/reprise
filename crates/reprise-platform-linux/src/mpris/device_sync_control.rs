//! Reprise-specific local D-Bus surface for live device synchronization.

use zbus::interface;

use reprise_core::agent_device_sync::{
    agent_device_sync_request, read_agent_device_sync_state, AgentDeviceSyncBlocker,
    AgentDeviceSyncChanges, AgentDeviceSyncCommand, AgentDeviceSyncControls, AgentDeviceSyncDevice,
    AgentDeviceSyncPhase, AgentDeviceSyncPlaylist, AgentDeviceSyncRequest, AgentDeviceSyncStorage,
    AgentDeviceSyncStorageAccess, AgentDeviceSyncStorageComposition,
    AgentDeviceSyncStorageKnowledge, AgentDeviceSyncStorageState, AgentDeviceSyncWarning,
    SharedAgentDeviceSyncState,
};
use reprise_core::device_sync::{SelectionSource, TransferProfile};

use reprise_runtime_protocol::device_sync::{
    DeviceChangeCounts, DeviceControls, DeviceProgress, DeviceSnapshot, DeviceSourceSelection,
    DeviceSourceSnapshot, DeviceStorageComposition, DeviceStorageSnapshot,
};
use reprise_runtime_protocol::PROTOCOL_VERSION;

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
    /// The contract version this service speaks. A client checks it before
    /// decoding anything else and refuses a foreign major version instead of
    /// misreading a payload it does not understand.
    #[zbus(property)]
    fn protocol_version(&self) -> (u32, u32) {
        (PROTOCOL_VERSION.major, PROTOCOL_VERSION.minor)
    }

    fn snapshot(&self) -> Vec<DeviceSnapshot> {
        read_agent_device_sync_state(&self.state)
            .devices
            .into_iter()
            .map(device_snapshot)
            .collect()
    }

    fn configure(
        &self,
        device_name: &str,
        sources: Vec<DeviceSourceSelection>,
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

fn device_snapshot(device: AgentDeviceSyncDevice) -> DeviceSnapshot {
    DeviceSnapshot {
        name: device.name,
        connected: device.connected,
        profile: device.profile.storage_value().to_owned(),
        managed_tracks: count(device.managed_tracks),
        unique_track_count: count(device.unique_track_count),
        target_bytes: device.target_bytes,
        sources: device.playlists.into_iter().map(source_snapshot).collect(),
        changes: change_counts(&device.changes),
        storage: storage_snapshot(&device.storage),
        blockers: device.blockers.into_iter().map(blocker_name).collect(),
        warnings: device.warnings.into_iter().map(warning_name).collect(),
        controls: controls(device.controls),
        phase: phase_name(&device.phase).to_owned(),
        progress: DeviceProgress {
            bytes_done: device.bytes_done,
            bytes_total: device.bytes_total,
            bytes_per_second: device.bytes_per_second,
        },
        current_track: device.current_track,
        last_synced_at: device.last_synced_at,
    }
}

fn source_snapshot(source: AgentDeviceSyncPlaylist) -> DeviceSourceSnapshot {
    let (kind, id) = source_parts(&source.source);
    DeviceSourceSnapshot {
        kind: kind.to_owned(),
        id,
        name: source.name,
        selected: source.selected,
        available: source.available,
        entry_count: count(source.entry_count),
        unique_track_count: count(source.unique_track_count),
        unavailable_count: count(source.unavailable_count),
        target_bytes: source.target_bytes,
        last_synced_at: source.last_synced_at,
    }
}

fn change_counts(changes: &AgentDeviceSyncChanges) -> DeviceChangeCounts {
    DeviceChangeCounts {
        additions: count(changes.additions),
        replacements: count(changes.replacements),
        removals: count(changes.removals),
        retained_unavailable: count(changes.retained_unavailable),
        playlist_writes: count(changes.playlist_writes),
        playlist_removals: count(changes.playlist_removals),
        transfer_bytes: changes.transfer_bytes,
    }
}

fn storage_snapshot(storage: &AgentDeviceSyncStorage) -> DeviceStorageSnapshot {
    let (state, shortfall_bytes) = storage_state(storage.state);
    DeviceStorageSnapshot {
        target_name: storage.target_name.clone(),
        state: state.to_owned(),
        shortfall_bytes,
        transfer_bytes: storage.transfer_bytes,
        current: storage_composition(&storage.current),
        after_sync: storage.after_sync.as_ref().map(storage_composition),
        access: storage_access_name(storage.access).to_owned(),
    }
}

fn storage_access_name(access: AgentDeviceSyncStorageAccess) -> &'static str {
    match access {
        AgentDeviceSyncStorageAccess::Writable => "writable",
        AgentDeviceSyncStorageAccess::ReadOnly => "read_only",
        AgentDeviceSyncStorageAccess::Unknown => "unknown",
    }
}

fn storage_composition(
    composition: &AgentDeviceSyncStorageComposition,
) -> DeviceStorageComposition {
    DeviceStorageComposition {
        total_bytes: composition.total_bytes,
        reprise_music_bytes: composition.reprise_music_bytes,
        other_music_bytes: composition.other_music_bytes,
        other_used_bytes: composition.other_used_bytes,
        free_bytes: composition.free_bytes,
        knowledge: match composition.knowledge {
            AgentDeviceSyncStorageKnowledge::Complete => "complete",
            AgentDeviceSyncStorageKnowledge::CapacityUnknown => "capacity_unknown",
            AgentDeviceSyncStorageKnowledge::Inconsistent => "inconsistent",
        }
        .to_owned(),
    }
}

fn controls(controls: AgentDeviceSyncControls) -> DeviceControls {
    DeviceControls {
        editable: controls.editable,
        can_start: controls.can_start,
        can_cancel: controls.can_cancel,
        can_eject: controls.can_eject,
    }
}

fn storage_state(state: AgentDeviceSyncStorageState) -> (&'static str, Option<u64>) {
    match state {
        AgentDeviceSyncStorageState::Fits => ("fits", None),
        AgentDeviceSyncStorageState::Insufficient { shortfall_bytes } => {
            ("insufficient", Some(shortfall_bytes))
        }
        AgentDeviceSyncStorageState::CapacityUnknown => ("capacity_unknown", None),
        AgentDeviceSyncStorageState::Inconsistent => ("inconsistent", None),
        AgentDeviceSyncStorageState::Blocked => ("blocked", None),
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

fn decode_source(selection: DeviceSourceSelection) -> zbus::fdo::Result<SelectionSource> {
    let DeviceSourceSelection { kind, id } = selection;
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

        let devices = control.snapshot();
        let device = &devices[0];
        assert_eq!(device.name, "Pixel");
        assert_eq!(device.profile, "original");
        assert_eq!(device.unique_track_count, 200);
        assert_eq!(device.sources[0].kind, "smart");
        assert_eq!(device.sources[0].id, 7);
        assert_eq!(device.sources[0].entry_count, 220);
        assert_eq!(device.sources[0].last_synced_at, Some(1_721_234_567));
        assert_eq!(device.changes.additions, 125);
        assert_eq!(device.storage.current.free_bytes, Some(80));
        assert!(device.controls.can_cancel);
        assert!(!device.controls.can_eject);
        assert_eq!(device.progress.bytes_per_second, 12);
        assert_eq!(device.last_synced_at, Some(1_721_234_890));
        assert_eq!(
            control.protocol_version(),
            (PROTOCOL_VERSION.major, PROTOCOL_VERSION.minor)
        );

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
                vec![
                    DeviceSourceSelection {
                        kind: "playlist".into(),
                        id: 3,
                    },
                    DeviceSourceSelection {
                        kind: "smart".into(),
                        id: 7,
                    },
                ],
                "opus_160",
            )
            .unwrap();
        responder.join().unwrap();
    }

    #[test]
    fn configure_rejects_invalid_source_identity_before_dispatch() {
        assert!(decode_source(DeviceSourceSelection {
            kind: "playlist".into(),
            id: 0,
        })
        .is_err());
        assert!(decode_source(DeviceSourceSelection {
            kind: "unknown".into(),
            id: 1,
        })
        .is_err());
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
