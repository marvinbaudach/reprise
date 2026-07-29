//! Blocking D-Bus client for the running app's live device-sync surface.

use crate::device_dto::{
    DeviceSyncBalanceDto, DeviceSyncCategoryDto, DeviceSyncChangesDto, DeviceSyncControlsDto,
    DeviceSyncDeviceDto, DeviceSyncParams, DeviceSyncPlaylistDto, DeviceSyncProgressDto,
    DeviceSyncSourceParam, DeviceSyncStateDto, DeviceSyncStorageCompositionDto,
    DeviceSyncStorageDto,
};
use crate::playback::PlaybackError;

const BUS_NAME: &str = "org.mpris.MediaPlayer2.reprise";
const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const DEVICE_SYNC_INTERFACE: &str = "org.reprise.DeviceSync1";

/// Wire shape for `CategorySnapshot` — mirrors
/// `reprise_platform_linux::mpris::device_sync_control::DeviceSyncCategoryRow`
/// field for field: kind, target_path, target_enabled, size_on_device_bytes,
/// has_cap, cap_bytes, reading_kind, files_to_copy, bytes_to_copy,
/// files_to_remove, bytes_freed, files_waiting_for_download,
/// playlists_rewritten.
type DeviceSyncCategoryRow = (
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
type DeviceSyncCategoryDeviceRow = (String, Vec<DeviceSyncCategoryRow>);
type DeviceSyncSourceSelection = (String, i64);
type DeviceSyncSourceRow = (
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
type DeviceSyncChangesRow = (u64, u64, u64, u64, u64, u64, u64);
type DeviceSyncStorageCompositionRow = (bool, u64, u64, u64, bool, u64, bool, u64, String);
type DeviceSyncStorageRow = (
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
type DeviceSyncControlsRow = (bool, bool, bool, bool);
type DeviceSyncProgressRow = (u64, u64, u64);
type DeviceSyncTimestampRow = (bool, i64);
type DeviceSyncRow = (
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceSyncAction {
    Configure {
        device_name: String,
        sources: Vec<DeviceSyncSourceSelection>,
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

fn source_selection(source: &DeviceSyncSourceParam) -> Result<DeviceSyncSourceSelection, String> {
    if source.id <= 0 {
        return Err("source id must be positive".into());
    }
    match source.kind.as_str() {
        "playlist" | "smart" => Ok((source.kind.clone(), source.id)),
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
    let rows: Vec<DeviceSyncRow> = proxy
        .call("Snapshot", &())
        .map_err(|error| map_zbus_error(&error))?;
    let categories: Vec<DeviceSyncCategoryDeviceRow> = proxy
        .call("CategorySnapshot", &())
        .map_err(|error| map_zbus_error(&error))?;
    let mut categories: std::collections::HashMap<String, Vec<DeviceSyncCategoryRow>> =
        categories.into_iter().collect();
    Ok(DeviceSyncStateDto {
        devices: rows
            .into_iter()
            .map(|row| {
                let device_categories = categories.remove(&row.0).unwrap_or_default();
                map_row(row, device_categories)
            })
            .collect(),
    })
}

fn map_row(row: DeviceSyncRow, category_rows: Vec<DeviceSyncCategoryRow>) -> DeviceSyncDeviceDto {
    let (
        name,
        connected,
        profile,
        managed_tracks,
        unique_track_count,
        target_bytes,
        playlists,
        changes,
        storage,
        blockers,
        warnings,
        controls,
        phase,
        progress,
        current_track,
        last_synced_at,
    ) = row;
    DeviceSyncDeviceDto {
        name,
        connected,
        last_synced_at: last_synced_at.0.then_some(last_synced_at.1),
        profile,
        managed_tracks,
        unique_track_count,
        target_bytes,
        playlists: playlists.into_iter().map(map_source_row).collect(),
        changes: map_changes_row(changes),
        storage: map_storage_row(storage),
        blockers,
        warnings,
        controls: DeviceSyncControlsDto {
            editable: controls.0,
            can_start: controls.1,
            can_cancel: controls.2,
            can_eject: controls.3,
        },
        phase,
        progress: DeviceSyncProgressDto {
            bytes_done: progress.0,
            bytes_total: progress.1,
            bytes_per_second: progress.2,
        },
        current_track,
        balance: balance_dto(&category_rows),
        categories: category_rows.into_iter().map(map_category_row).collect(),
    }
}

fn decode_reading(row: &DeviceSyncCategoryRow) -> reprise_core::device_sync::CategoryReading {
    use reprise_core::device_sync::{CategoryDiff, CategoryReading};
    match row.6.as_str() {
        "source_off" => CategoryReading::SourceOff,
        "unavailable_kept_on_phone" => CategoryReading::UnavailableKeptOnPhone,
        _ => CategoryReading::Diff(CategoryDiff {
            files_to_copy: row.7 as usize,
            bytes_to_copy: row.8,
            files_to_remove: row.9 as usize,
            bytes_freed: row.10,
            files_waiting_for_download: row.11 as usize,
            playlists_rewritten: row.12 as usize,
        }),
    }
}

fn map_category_row(row: DeviceSyncCategoryRow) -> DeviceSyncCategoryDto {
    use reprise_core::device_sync::CategoryReading;
    let reading = decode_reading(&row);
    let (kind, target_path, target_enabled, size_on_device_bytes, has_cap, cap_bytes, ..) = row;
    let (reading_name, diff) = match reading {
        CategoryReading::Diff(diff) => ("diff", diff),
        CategoryReading::SourceOff => ("source_off", Default::default()),
        CategoryReading::UnavailableKeptOnPhone => {
            ("unavailable_kept_on_phone", Default::default())
        }
    };
    DeviceSyncCategoryDto {
        kind: static_kind_name(&kind),
        target_path,
        target_enabled,
        size_on_device_bytes,
        cap_bytes: has_cap.then_some(cap_bytes),
        reading: reading_name,
        files_to_copy: diff.files_to_copy as u64,
        bytes_to_copy: diff.bytes_to_copy,
        files_to_remove: diff.files_to_remove as u64,
        bytes_freed: diff.bytes_freed,
        files_waiting_for_download: diff.files_waiting_for_download as u64,
        playlists_rewritten: diff.playlists_rewritten as u64,
    }
}

fn static_kind_name(kind: &str) -> &'static str {
    match kind {
        "youtube_audio" => "youtube_audio",
        "podcast_episodes" => "podcast_episodes",
        _ => "playlists",
    }
}

/// `MTP-22`: the aggregate balance across every category currently reading
/// a computed diff, via the exact same `reprise_core::device_sync::
/// aggregate_balance` the GTK device page's sidebar-card tooltip reads —
/// not a re-derived sum.
fn balance_dto(rows: &[DeviceSyncCategoryRow]) -> DeviceSyncBalanceDto {
    let readings = rows.iter().map(decode_reading).collect::<Vec<_>>();
    let balance = reprise_core::device_sync::aggregate_balance(&readings);
    DeviceSyncBalanceDto {
        files_to_copy: balance.files_to_copy as u64,
        bytes_to_copy: balance.bytes_to_copy,
        files_to_remove: balance.files_to_remove as u64,
        bytes_freed: balance.bytes_freed,
        files_waiting_for_download: balance.files_waiting_for_download as u64,
        playlists_rewritten: balance.playlists_rewritten as u64,
        has_work: balance.has_work(),
    }
}

fn map_source_row(row: DeviceSyncSourceRow) -> DeviceSyncPlaylistDto {
    DeviceSyncPlaylistDto {
        kind: row.0,
        id: row.1,
        name: row.2.then_some(row.3),
        selected: row.4,
        available: row.5,
        entry_count: row.6,
        unique_track_count: row.7,
        unavailable_count: row.8,
        target_bytes: row.9,
        last_synced_at: row.10.then_some(row.11),
    }
}

fn map_changes_row(row: DeviceSyncChangesRow) -> DeviceSyncChangesDto {
    DeviceSyncChangesDto {
        additions: row.0,
        replacements: row.1,
        removals: row.2,
        retained_unavailable: row.3,
        playlist_writes: row.4,
        playlist_removals: row.5,
        transfer_bytes: row.6,
    }
}

fn map_storage_row(row: DeviceSyncStorageRow) -> DeviceSyncStorageDto {
    DeviceSyncStorageDto {
        target_name: row.0.then_some(row.1),
        access: row.9,
        state: row.2,
        shortfall_bytes: row.3.then_some(row.4),
        transfer_bytes: row.5,
        current: map_storage_composition_row(row.6),
        after_sync: row.7.then(|| map_storage_composition_row(row.8)),
    }
}

fn map_storage_composition_row(
    row: DeviceSyncStorageCompositionRow,
) -> DeviceSyncStorageCompositionDto {
    DeviceSyncStorageCompositionDto {
        total_bytes: row.0.then_some(row.1),
        reprise_music_bytes: row.2,
        other_music_bytes: row.3,
        other_used_bytes: row.4.then_some(row.5),
        free_bytes: row.6.then_some(row.7),
        knowledge: row.8,
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
    fn row_mapping_preserves_the_compact_mirror_page_without_paths_or_serials() {
        let dto = map_row(
            (
                "Pixel".into(),
                true,
                "original".into(),
                75,
                200,
                80,
                vec![(
                    "smart".into(),
                    7,
                    true,
                    "Heavy rotation".into(),
                    true,
                    true,
                    220,
                    200,
                    2,
                    80,
                    true,
                    1_721_234_567,
                )],
                (125, 5, 0, 2, 1, 0, 60),
                (
                    true,
                    "Internal storage".into(),
                    "fits".into(),
                    false,
                    0,
                    60,
                    (true, 100, 20, 10, true, 30, true, 40, "complete".into()),
                    true,
                    (true, 100, 80, 10, true, 10, true, 0, "complete".into()),
                    "writable".into(),
                ),
                Vec::new(),
                vec!["unavailable_not_on_device".into()],
                (false, false, true, false),
                "copying".into(),
                (20, 60, 10),
                "Sun//Eater — Lorna Shore".into(),
                (true, 1_721_234_890),
            ),
            Vec::new(),
        );

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
        assert!(dto.categories.is_empty());
        assert!(!dto.balance.has_work);
        let json = serde_json::to_value(dto).unwrap();
        assert!(json.get("serial").is_none());
    }

    /// Block H (MCP parity): the three category readings must survive the
    /// wire distinctly and roll up into `MTP-22`'s aggregate balance via
    /// `reprise_core::device_sync::aggregate_balance` — not a hand-summed
    /// duplicate that could silently drift from it.
    #[test]
    fn category_rows_decode_into_distinct_readings_and_an_aggregate_balance() {
        let categories: Vec<DeviceSyncCategoryRow> = vec![
            (
                "youtube_audio".into(),
                "/Music/Reprise-YouTube".into(),
                true,
                42,
                true,
                8 * 1024 * 1024 * 1024,
                "diff".into(),
                3,
                900,
                1,
                50,
                2,
                0,
            ),
            (
                "podcast_episodes".into(),
                "/Podcasts/Reprise".into(),
                true,
                0,
                true,
                4 * 1024 * 1024 * 1024,
                "source_off".into(),
                0,
                0,
                0,
                0,
                0,
                0,
            ),
            (
                "playlists".into(),
                "/Music/Reprise".into(),
                true,
                0,
                false,
                0,
                "unavailable_kept_on_phone".into(),
                0,
                0,
                0,
                0,
                0,
                0,
            ),
        ];

        let dtos = categories
            .iter()
            .cloned()
            .map(map_category_row)
            .collect::<Vec<_>>();

        assert_eq!(dtos[0].kind, "youtube_audio");
        assert_eq!(dtos[0].reading, "diff");
        assert_eq!(dtos[0].files_to_copy, 3);
        assert_eq!(dtos[0].bytes_freed, 50);
        assert_eq!(dtos[0].cap_bytes, Some(8 * 1024 * 1024 * 1024));
        assert_eq!(dtos[0].target_path, "/Music/Reprise-YouTube");

        assert_eq!(dtos[1].reading, "source_off");
        assert_ne!(
            dtos[1].reading, dtos[2].reading,
            "source_off and unavailable_kept_on_phone must not collapse into the same string"
        );
        assert_eq!(dtos[2].cap_bytes, None, "no cap must serialize as absent");

        let balance = balance_dto(&categories);
        assert_eq!(
            balance.files_to_copy, 3,
            "only the diff category contributes to the balance"
        );
        assert_eq!(balance.bytes_freed, 50);
        assert!(balance.has_work);

        let json = serde_json::to_value(&dtos).unwrap();
        assert!(
            json.to_string().contains("Music/Reprise-YouTube"),
            "the device's own MTP target path is intentionally shown, matching MTP-28 in the GUI"
        );
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
                sources: vec![("playlist".into(), 3), ("smart".into(), 7)],
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
