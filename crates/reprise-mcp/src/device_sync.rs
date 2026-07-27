//! Blocking D-Bus client for the running app's live device-sync surface.

use crate::device_dto::{DeviceSyncDeviceDto, DeviceSyncParams, DeviceSyncStateDto};
use crate::playback::PlaybackError;

const BUS_NAME: &str = "org.mpris.MediaPlayer2.reprise";
const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const DEVICE_SYNC_INTERFACE: &str = "org.reprise.DeviceSync1";
type DeviceSyncRow = (
    String,
    bool,
    bool,
    u64,
    bool,
    u64,
    u64,
    u64,
    u64,
    u64,
    u64,
    String,
    u64,
    u64,
    u64,
    String,
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceSyncAction {
    ConfigurePlaylist {
        device_name: String,
        playlist_name: String,
        remove_unselected: bool,
        bitrate_kbps: u32,
    },
    Start {
        device_name: String,
    },
    Cancel {
        device_name: String,
    },
}

impl DeviceSyncAction {
    pub fn from_params(params: &DeviceSyncParams) -> Result<Self, String> {
        let device_name = required_text(&params.device_name, "device_name")?;
        match params.action.as_str() {
            "configure_playlist" => {
                let bitrate_kbps = params.bitrate_kbps.unwrap_or(256);
                if reprise_core::device_sync::Mp3Quality::try_from(bitrate_kbps).is_err() {
                    return Err(format!(
                        "bitrate_kbps must be one of 128, 192, 256, 320; got {bitrate_kbps}"
                    ));
                }
                Ok(Self::ConfigurePlaylist {
                    device_name,
                    playlist_name: required_text(
                        params.playlist_name.as_deref().unwrap_or_default(),
                        "playlist_name",
                    )?,
                    remove_unselected: params.remove_unselected.unwrap_or(false),
                    bitrate_kbps,
                })
            }
            "start" => Ok(Self::Start { device_name }),
            "cancel" => Ok(Self::Cancel { device_name }),
            other => Err(format!("unknown action '{other}'")),
        }
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
    Ok(DeviceSyncStateDto {
        devices: rows.into_iter().map(map_row).collect(),
    })
}

fn map_row(row: DeviceSyncRow) -> DeviceSyncDeviceDto {
    DeviceSyncDeviceDto {
        name: row.0,
        connected: row.1,
        available_bytes: row.2.then_some(row.3),
        total_bytes: row.4.then_some(row.5),
        managed_tracks: row.6,
        selected_tracks: row.7,
        tracks_to_copy: row.8,
        tracks_to_remove: row.9,
        bytes_to_copy: row.10,
        phase: row.11,
        bytes_done: row.12,
        bytes_total: row.13,
        bytes_per_second: row.14,
        current_track: row.15,
    }
}

pub fn mutate(action: DeviceSyncAction) -> Result<String, PlaybackError> {
    let proxy = connect()?;
    let summary = match action {
        DeviceSyncAction::ConfigurePlaylist {
            device_name,
            playlist_name,
            remove_unselected,
            bitrate_kbps,
        } => {
            let _: () = proxy
                .call(
                    "ConfigurePlaylist",
                    &(
                        &device_name,
                        &playlist_name,
                        remove_unselected,
                        bitrate_kbps,
                    ),
                )
                .map_err(|error| map_zbus_error(&error))?;
            format!(
                "Configured {device_name} from playlist '{playlist_name}' \
                 (remove_unselected={remove_unselected}, bitrate_kbps={bitrate_kbps})"
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
    };
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_mapping_preserves_capacity_and_rate_without_internal_identity() {
        let dto = map_row((
            "Pixel".into(),
            true,
            true,
            80,
            true,
            100,
            75,
            200,
            125,
            0,
            60,
            "copying".into(),
            20,
            60,
            10,
            "Sun//Eater — Lorna Shore".into(),
        ));

        assert_eq!(dto.available_bytes, Some(80));
        assert_eq!(dto.selected_tracks, 200);
        assert_eq!(dto.bytes_per_second, 10);
        let json = serde_json::to_value(dto).unwrap();
        assert!(json.get("serial").is_none());
        assert!(json.get("path").is_none());
    }

    #[test]
    fn configure_is_explicit_about_destructive_selection_cleanup() {
        let action = DeviceSyncAction::from_params(&DeviceSyncParams {
            action: "configure_playlist".into(),
            device_name: "Pixel".into(),
            playlist_name: Some("Lorna Shore & Similar".into()),
            remove_unselected: Some(true),
            bitrate_kbps: Some(320),
        })
        .unwrap();
        assert_eq!(
            action,
            DeviceSyncAction::ConfigurePlaylist {
                device_name: "Pixel".into(),
                playlist_name: "Lorna Shore & Similar".into(),
                remove_unselected: true,
                bitrate_kbps: 320,
            }
        );
    }

    #[test]
    fn configure_rejects_an_unsupported_bitrate_before_bus_dispatch() {
        let error = DeviceSyncAction::from_params(&DeviceSyncParams {
            action: "configure_playlist".into(),
            device_name: "Pixel".into(),
            playlist_name: Some("Lorna Shore & Similar".into()),
            remove_unselected: Some(true),
            bitrate_kbps: Some(160),
        })
        .unwrap_err();

        assert!(error.contains("bitrate_kbps must be one of"));
        assert!(error.contains("got 160"));
    }

    #[test]
    fn configure_defaults_to_mp3_256_quality() {
        let action = DeviceSyncAction::from_params(&DeviceSyncParams {
            action: "configure_playlist".into(),
            device_name: "Pixel".into(),
            playlist_name: Some("Road".into()),
            remove_unselected: None,
            bitrate_kbps: None,
        })
        .unwrap();

        assert_eq!(
            action,
            DeviceSyncAction::ConfigurePlaylist {
                device_name: "Pixel".into(),
                playlist_name: "Road".into(),
                remove_unselected: false,
                bitrate_kbps: 256,
            }
        );
    }
}
