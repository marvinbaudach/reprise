//! Blocking D-Bus client for the running app's live device-sync surface.

use crate::device_dto::{
    DeviceSyncChangesDto, DeviceSyncControlsDto, DeviceSyncDeviceDto, DeviceSyncParams,
    DeviceSyncPlaylistDto, DeviceSyncProgressDto, DeviceSyncSourceParam, DeviceSyncStateDto,
    DeviceSyncStorageCompositionDto, DeviceSyncStorageDto,
};
use crate::playback::PlaybackError;

const BUS_NAME: &str = "org.mpris.MediaPlayer2.reprise";
const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const DEVICE_SYNC_INTERFACE: &str = "org.reprise.DeviceSync1";

use reprise_runtime_protocol::device_sync::{
    DeviceChangeCounts, DeviceSnapshot, DeviceSourceSelection, DeviceSourceSnapshot,
    DeviceStorageComposition, DeviceStorageSnapshot,
};
use reprise_runtime_protocol::{ProtocolVersion, PROTOCOL_VERSION};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceSyncAction {
    Configure {
        device_name: String,
        sources: Vec<DeviceSourceSelection>,
        profile: String,
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

impl DeviceSyncAction {
    pub fn from_params(params: &DeviceSyncParams) -> Result<Self, String> {
        let device_name = required_text(&params.device_name, "device_name")?;
        match params.action.as_str() {
            "configure" => {
                let profile = params.profile.as_deref().unwrap_or("opus_160");
                let profile =
                    reprise_core::device_sync::TransferProfile::from_storage_value(profile)
                        .ok_or_else(|| {
                            format!(
                                "profile must be one of opus_160, mp3_256, original; got {profile}"
                            )
                        })?
                        .storage_value()
                        .to_owned();
                let sources = params
                    .sources
                    .as_ref()
                    .ok_or_else(|| "sources is required for configure".to_owned())?
                    .iter()
                    .map(source_selection)
                    .collect::<Result<Vec<_>, _>>()?;
                let unique = sources
                    .iter()
                    .cloned()
                    .collect::<std::collections::HashSet<_>>();
                if unique.len() != sources.len() {
                    return Err("sources must not contain duplicates".into());
                }
                Ok(Self::Configure {
                    device_name,
                    sources,
                    profile,
                })
            }
            "start" => {
                reject_configuration_fields(params)?;
                Ok(Self::Start { device_name })
            }
            "cancel" => {
                reject_configuration_fields(params)?;
                Ok(Self::Cancel { device_name })
            }
            "eject" => {
                reject_configuration_fields(params)?;
                Ok(Self::Eject { device_name })
            }
            other => Err(format!("unknown action '{other}'")),
        }
    }
}

fn reject_configuration_fields(params: &DeviceSyncParams) -> Result<(), String> {
    if params.sources.is_some() || params.profile.is_some() {
        Err("sources and profile are only valid for configure".into())
    } else {
        Ok(())
    }
}

fn source_selection(source: &DeviceSyncSourceParam) -> Result<DeviceSourceSelection, String> {
    if source.id <= 0 {
        return Err("source id must be positive".into());
    }
    match source.kind.as_str() {
        "playlist" | "smart" => Ok(DeviceSourceSelection {
            kind: source.kind.clone(),
            id: source.id,
        }),
        other => Err(format!(
            "source kind must be 'playlist' or 'smart'; got '{other}'"
        )),
    }
}

fn required_text(value: &str, field: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        Err(format!("{field} must not be empty"))
    } else {
        Ok(value.to_owned())
    }
}

fn connect() -> Result<zbus::blocking::Proxy<'static>, PlaybackError> {
    let connection = zbus::blocking::Connection::session()
        .map_err(|error| PlaybackError::Bus(format!("no D-Bus session bus available: {error}")))?;
    zbus::blocking::Proxy::new(&connection, BUS_NAME, OBJECT_PATH, DEVICE_SYNC_INTERFACE)
        .map_err(|error| map_zbus_error(&error))
}

fn map_zbus_error(error: &zbus::Error) -> PlaybackError {
    if let zbus::Error::MethodError(name, _, _) = error {
        if matches!(
            name.as_str(),
            "org.freedesktop.DBus.Error.ServiceUnknown"
                | "org.freedesktop.DBus.Error.NameHasNoOwner"
        ) {
            return PlaybackError::NoPlayer;
        }
    }
    PlaybackError::Bus(error.to_string())
}

pub fn state() -> Result<DeviceSyncStateDto, PlaybackError> {
    let proxy = connect()?;
    check_protocol_version(&proxy)?;
    let devices: Vec<DeviceSnapshot> = proxy
        .call("Snapshot", &())
        .map_err(|error| map_zbus_error(&error))?;
    Ok(DeviceSyncStateDto {
        devices: devices.into_iter().map(map_device).collect(),
    })
}

/// Refuses a service whose contract this build cannot read, instead of
/// decoding a payload it does not understand. Section 9.7 of
/// `docs/plans/multi-frontend-core.md` calls this the `Refused` category; it
/// borrows `PlaybackError::Bus` until Stage 3 gives the runtime client its
/// own error type.
fn check_protocol_version(proxy: &zbus::blocking::Proxy<'static>) -> Result<(), PlaybackError> {
    let (major, minor): (u32, u32) = proxy
        .get_property("ProtocolVersion")
        .map_err(|error| map_zbus_error(&error))?;
    let served = ProtocolVersion { major, minor };
    if served.is_compatible_with(PROTOCOL_VERSION) {
        return Ok(());
    }
    Err(version_mismatch(served))
}

fn version_mismatch(served: ProtocolVersion) -> PlaybackError {
    PlaybackError::Bus(format!(
        "the running Reprise speaks device-sync protocol {served}, this build needs \
         {PROTOCOL_VERSION}; restart the app so both sides match"
    ))
}

fn map_device(device: DeviceSnapshot) -> DeviceSyncDeviceDto {
    DeviceSyncDeviceDto {
        name: device.name,
        connected: device.connected,
        last_synced_at: device.last_synced_at,
        profile: device.profile,
        managed_tracks: device.managed_tracks,
        unique_track_count: device.unique_track_count,
        target_bytes: device.target_bytes,
        playlists: device.sources.into_iter().map(map_source).collect(),
        changes: map_changes(&device.changes),
        storage: map_storage(device.storage),
        blockers: device.blockers,
        warnings: device.warnings,
        controls: DeviceSyncControlsDto {
            editable: device.controls.editable,
            can_start: device.controls.can_start,
            can_cancel: device.controls.can_cancel,
            can_eject: device.controls.can_eject,
        },
        phase: device.phase,
        progress: DeviceSyncProgressDto {
            bytes_done: device.progress.bytes_done,
            bytes_total: device.progress.bytes_total,
            bytes_per_second: device.progress.bytes_per_second,
        },
        current_track: device.current_track,
    }
}

fn map_source(source: DeviceSourceSnapshot) -> DeviceSyncPlaylistDto {
    DeviceSyncPlaylistDto {
        kind: source.kind,
        id: source.id,
        name: source.name,
        selected: source.selected,
        available: source.available,
        entry_count: source.entry_count,
        unique_track_count: source.unique_track_count,
        unavailable_count: source.unavailable_count,
        target_bytes: source.target_bytes,
        last_synced_at: source.last_synced_at,
    }
}

fn map_changes(changes: &DeviceChangeCounts) -> DeviceSyncChangesDto {
    DeviceSyncChangesDto {
        additions: changes.additions,
        replacements: changes.replacements,
        removals: changes.removals,
        retained_unavailable: changes.retained_unavailable,
        playlist_writes: changes.playlist_writes,
        playlist_removals: changes.playlist_removals,
        transfer_bytes: changes.transfer_bytes,
    }
}

fn map_storage(storage: DeviceStorageSnapshot) -> DeviceSyncStorageDto {
    DeviceSyncStorageDto {
        target_name: storage.target_name,
        access: storage.access,
        state: storage.state,
        shortfall_bytes: storage.shortfall_bytes,
        transfer_bytes: storage.transfer_bytes,
        current: map_storage_composition(storage.current),
        after_sync: storage.after_sync.map(map_storage_composition),
    }
}

fn map_storage_composition(
    composition: DeviceStorageComposition,
) -> DeviceSyncStorageCompositionDto {
    DeviceSyncStorageCompositionDto {
        total_bytes: composition.total_bytes,
        reprise_music_bytes: composition.reprise_music_bytes,
        other_music_bytes: composition.other_music_bytes,
        other_used_bytes: composition.other_used_bytes,
        free_bytes: composition.free_bytes,
        knowledge: composition.knowledge,
    }
}

pub fn mutate(action: DeviceSyncAction) -> Result<String, PlaybackError> {
    let proxy = connect()?;
    let summary = match action {
        DeviceSyncAction::Configure {
            device_name,
            sources,
            profile,
        } => {
            let _: () = proxy
                .call("Configure", &(&device_name, &sources, &profile))
                .map_err(|error| map_zbus_error(&error))?;
            format!(
                "Configured {device_name} to mirror {} playlist source(s) with profile {profile}",
                sources.len()
            )
        }
        DeviceSyncAction::Start { device_name } => {
            let _: () = proxy
                .call("Start", &(&device_name,))
                .map_err(|error| map_zbus_error(&error))?;
            format!("Queued synchronization for {device_name}")
        }
        DeviceSyncAction::Cancel { device_name } => {
            let _: () = proxy
                .call("Cancel", &(&device_name,))
                .map_err(|error| map_zbus_error(&error))?;
            format!("Requested synchronization cancellation for {device_name}")
        }
        DeviceSyncAction::Eject { device_name } => {
            let _: () = proxy
                .call("Eject", &(&device_name,))
                .map_err(|error| map_zbus_error(&error))?;
            format!("Requested ejection for {device_name}")
        }
    };
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_mapping_preserves_the_compact_mirror_page_without_paths_or_serials() {
        let dto = map_device(DeviceSnapshot {
            name: "Pixel".into(),
            connected: true,
            profile: "original".into(),
            managed_tracks: 75,
            unique_track_count: 200,
            target_bytes: 80,
            sources: vec![DeviceSourceSnapshot {
                kind: "smart".into(),
                id: 7,
                name: Some("Heavy rotation".into()),
                selected: true,
                available: true,
                entry_count: 220,
                unique_track_count: 200,
                unavailable_count: 2,
                target_bytes: 80,
                last_synced_at: Some(1_721_234_567),
            }],
            changes: DeviceChangeCounts {
                additions: 125,
                replacements: 5,
                removals: 0,
                retained_unavailable: 2,
                playlist_writes: 1,
                playlist_removals: 0,
                transfer_bytes: 60,
            },
            storage: DeviceStorageSnapshot {
                target_name: Some("Internal storage".into()),
                state: "fits".into(),
                shortfall_bytes: None,
                transfer_bytes: 60,
                current: DeviceStorageComposition {
                    total_bytes: Some(100),
                    reprise_music_bytes: 20,
                    other_music_bytes: 10,
                    other_used_bytes: Some(30),
                    free_bytes: Some(40),
                    knowledge: "complete".into(),
                },
                after_sync: Some(DeviceStorageComposition {
                    total_bytes: Some(100),
                    reprise_music_bytes: 80,
                    other_music_bytes: 10,
                    other_used_bytes: Some(10),
                    free_bytes: Some(0),
                    knowledge: "complete".into(),
                }),
                access: "writable".into(),
            },
            blockers: Vec::new(),
            warnings: vec!["unavailable_not_on_device".into()],
            controls: reprise_runtime_protocol::device_sync::DeviceControls {
                editable: false,
                can_start: false,
                can_cancel: true,
                can_eject: false,
            },
            phase: "copying".into(),
            progress: reprise_runtime_protocol::device_sync::DeviceProgress {
                bytes_done: 20,
                bytes_total: 60,
                bytes_per_second: 10,
            },
            current_track: "Sun//Eater — Lorna Shore".into(),
            last_synced_at: Some(1_721_234_890),
        });

        assert_eq!(dto.profile, "original");
        assert_eq!(dto.last_synced_at, Some(1_721_234_890));
        assert_eq!(dto.unique_track_count, 200);
        assert_eq!(dto.playlists[0].kind, "smart");
        assert_eq!(dto.playlists[0].entry_count, 220);
        assert_eq!(dto.playlists[0].last_synced_at, Some(1_721_234_567));
        assert_eq!(dto.changes.replacements, 5);
        assert_eq!(dto.storage.current.free_bytes, Some(40));
        assert_eq!(dto.storage.access, "writable");
        assert_eq!(dto.storage.after_sync.as_ref().unwrap().free_bytes, Some(0));
        assert!(dto.controls.can_cancel);
        assert_eq!(dto.progress.bytes_per_second, 10);
        let json = serde_json::to_value(dto).unwrap();
        assert!(json.get("serial").is_none());
        assert!(!json.to_string().contains("path"));
    }

    /// The point of the handshake is a message a person can act on, instead
    /// of a decode failure deep inside zvariant.
    #[test]
    fn a_foreign_protocol_version_is_refused_with_an_actionable_message() {
        let PlaybackError::Bus(message) = version_mismatch(ProtocolVersion { major: 2, minor: 0 })
        else {
            panic!("a version mismatch is a bus-level refusal");
        };
        assert!(message.contains("speaks device-sync protocol 2.0"));
        assert!(message.contains(&format!("needs {PROTOCOL_VERSION}")));
        assert!(message.contains("restart the app"));
    }

    #[test]
    fn configure_accepts_multiple_stable_sources_and_defaults_to_opus_160() {
        let action = DeviceSyncAction::from_params(&DeviceSyncParams {
            action: "configure".into(),
            device_name: "Pixel".into(),
            sources: Some(vec![
                DeviceSyncSourceParam {
                    kind: "playlist".into(),
                    id: 3,
                },
                DeviceSyncSourceParam {
                    kind: "smart".into(),
                    id: 7,
                },
            ]),
            profile: None,
        })
        .unwrap();
        assert_eq!(
            action,
            DeviceSyncAction::Configure {
                device_name: "Pixel".into(),
                sources: vec![
                    DeviceSourceSelection {
                        kind: "playlist".into(),
                        id: 3,
                    },
                    DeviceSourceSelection {
                        kind: "smart".into(),
                        id: 7,
                    },
                ],
                profile: "opus_160".into(),
            }
        );
    }

    #[test]
    fn configure_rejects_unsupported_profile_and_duplicate_or_invalid_sources() {
        let error = DeviceSyncAction::from_params(&DeviceSyncParams {
            action: "configure".into(),
            device_name: "Pixel".into(),
            sources: Some(Vec::new()),
            profile: Some("opus_320".into()),
        })
        .unwrap_err();
        assert!(error.contains("profile must be one of"));
        assert!(error.contains("got opus_320"));

        let duplicate = DeviceSyncSourceParam {
            kind: "playlist".into(),
            id: 3,
        };
        let error = DeviceSyncAction::from_params(&DeviceSyncParams {
            action: "configure".into(),
            device_name: "Pixel".into(),
            sources: Some(vec![duplicate.clone(), duplicate]),
            profile: Some("mp3_256".into()),
        })
        .unwrap_err();
        assert!(error.contains("duplicates"));

        let error = DeviceSyncAction::from_params(&DeviceSyncParams {
            action: "configure".into(),
            device_name: "Pixel".into(),
            sources: Some(vec![DeviceSyncSourceParam {
                kind: "podcast".into(),
                id: 1,
            }]),
            profile: Some("original".into()),
        })
        .unwrap_err();
        assert!(error.contains("playlist"));
        assert!(error.contains("smart"));
    }

    #[test]
    fn headless_actions_reject_configuration_only_fields() {
        for action in ["start", "cancel", "eject"] {
            let error = DeviceSyncAction::from_params(&DeviceSyncParams {
                action: action.into(),
                device_name: "Pixel".into(),
                sources: Some(Vec::new()),
                profile: None,
            })
            .unwrap_err();
            assert!(error.contains("only valid for configure"));
        }
    }
}
