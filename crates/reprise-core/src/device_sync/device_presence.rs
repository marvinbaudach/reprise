//! Pure projection of connected MTP devices onto Reprise's single session.

use std::collections::HashSet;

use chrono::{DateTime, Utc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetectedDevice {
    pub id: String,
    pub name: String,
}

/// Chooses the durable key for device-owned state. Provider values are
/// normalized here so every platform follows the same UUID-first rule and
/// a volatile transport URI can never be supplied as a fallback.
#[must_use]
pub fn stable_device_identity(uuid: Option<&str>, usb_serial: Option<&str>) -> Option<String> {
    uuid.into_iter()
        .chain(usb_serial)
        .map(str::trim)
        .find(|value| !value.is_empty())
        .map(str::to_owned)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceSessionState {
    Active,
    Inert { active_device_name: String },
    Remembered,
}

impl DeviceSessionState {
    #[must_use]
    pub const fn opens_session(&self) -> bool {
        matches!(self, Self::Active)
    }

    #[must_use]
    pub const fn offers_sync(&self) -> bool {
        self.opens_session()
    }

    #[must_use]
    pub const fn shows_diff(&self) -> bool {
        matches!(self, Self::Active)
    }

    #[must_use]
    pub fn status_text(&self) -> Option<String> {
        match self {
            Self::Active => None,
            Self::Inert { active_device_name } => Some(format!(
                "Plugged in · disconnect {active_device_name} to use it"
            )),
            Self::Remembered => Some("Not connected · never verified".to_string()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceSessionProjection {
    pub id: String,
    pub state: DeviceSessionState,
}

/// Keeps the existing session owner while it remains detected. Otherwise
/// the first detected device becomes the sole owner. The owner is projected
/// first so every frontend can preserve the same ordering without re-deciding
/// which device is active.
#[must_use]
pub fn project_device_sessions(
    previous_active_id: Option<&str>,
    devices: &[DetectedDevice],
) -> Vec<DeviceSessionProjection> {
    let Some(active) = previous_active_id
        .and_then(|id| devices.iter().find(|device| device.id == id))
        .or_else(|| devices.first())
    else {
        return Vec::new();
    };
    let mut projection = Vec::with_capacity(devices.len());
    projection.push(DeviceSessionProjection {
        id: active.id.clone(),
        state: DeviceSessionState::Active,
    });
    projection.extend(
        devices
            .iter()
            .filter(|device| device.id != active.id)
            .map(|device| DeviceSessionProjection {
                id: device.id.clone(),
                state: DeviceSessionState::Inert {
                    active_device_name: active.name.clone(),
                },
            }),
    );
    projection
}

/// Projects the complete device list in one stable order: the sole active
/// connection, any other detected-but-inert devices, then durable history.
/// A connected stable identity suppresses its remembered duplicate.
#[must_use]
pub fn project_device_presence(
    previous_active_id: Option<&str>,
    connected: &[DetectedDevice],
    remembered: &[DetectedDevice],
) -> Vec<DeviceSessionProjection> {
    let mut projection = project_device_sessions(previous_active_id, connected);
    let connected_ids = connected
        .iter()
        .map(|device| device.id.as_str())
        .collect::<HashSet<_>>();
    projection.extend(
        remembered
            .iter()
            .filter(|device| !connected_ids.contains(device.id.as_str()))
            .map(|device| DeviceSessionProjection {
                id: device.id.clone(),
                state: DeviceSessionState::Remembered,
            }),
    );
    projection
}

/// The only honest sidebar reading for absent hardware. No copy/remove
/// balance is accepted here because it would be a guess after unplugging.
#[must_use]
pub fn remembered_device_status(
    last_verified: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> String {
    let Some(verified) = last_verified else {
        return "Not connected · never verified".to_string();
    };
    let minutes = now.signed_duration_since(verified).num_minutes().max(0);
    let age = if minutes < 1 {
        "just now".to_string()
    } else if minutes < 60 {
        format!("{minutes} min ago")
    } else if minutes < 24 * 60 {
        let hours = minutes / 60;
        format!("{hours} {} ago", if hours == 1 { "hour" } else { "hours" })
    } else {
        let days = minutes / (24 * 60);
        format!("{days} {} ago", if days == 1 { "day" } else { "days" })
    };
    format!("Not connected · synced {age}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detected(id: &str, name: &str) -> DetectedDevice {
        DetectedDevice {
            id: id.to_string(),
            name: name.to_string(),
        }
    }

    #[test]
    fn mtp_48_only_the_first_detected_device_owns_the_single_open_session() {
        let first = vec![
            detected("anna", "Pixel 7a (Anna)"),
            detected("ben", "Pixel 7a (Ben)"),
        ];

        let projected = project_device_sessions(None, &first);

        assert_eq!(
            projected,
            vec![
                DeviceSessionProjection {
                    id: "anna".into(),
                    state: DeviceSessionState::Active,
                },
                DeviceSessionProjection {
                    id: "ben".into(),
                    state: DeviceSessionState::Inert {
                        active_device_name: "Pixel 7a (Anna)".into(),
                    },
                },
            ]
        );
        assert_eq!(
            projected
                .iter()
                .filter(|device| device.state.opens_session())
                .count(),
            1
        );
        assert_eq!(
            projected[1].state.status_text(),
            Some("Plugged in · disconnect Pixel 7a (Anna) to use it".into())
        );
        assert!(!projected[1].state.offers_sync());

        let reordered = vec![
            detected("ben", "Pixel 7a (Ben)"),
            detected("anna", "Pixel 7a (Anna)"),
        ];
        assert_eq!(
            project_device_sessions(Some("anna"), &reordered)[0].id,
            "anna",
            "enumeration order changes must not steal the live session"
        );

        assert_eq!(
            project_device_sessions(Some("anna"), &[detected("ben", "Pixel 7a (Ben)")])[0].state,
            DeviceSessionState::Active,
            "the waiting device becomes active once the owner disconnects"
        );
    }

    #[test]
    fn mtp_49_identity_prefers_uuid_then_usb_serial_and_never_uses_the_root_uri() {
        assert_eq!(
            stable_device_identity(Some(" mount-uuid "), Some("usb-serial")),
            Some("mount-uuid".into())
        );
        assert_eq!(
            stable_device_identity(None, Some(" usb-serial ")),
            Some("usb-serial".into())
        );
        assert_eq!(stable_device_identity(None, None), None);
        assert_eq!(
            stable_device_identity(Some("  "), Some("  ")),
            None,
            "blank provider values are not stable identities"
        );
        assert_ne!(
            stable_device_identity(None, None),
            Some("mtp://[usb:001,013]/".into()),
            "the volatile MTP root URI must never become a memory key"
        );
    }

    #[test]
    fn mtp_50_active_and_inert_devices_precede_dimmed_remembered_history_without_a_diff() {
        let connected = vec![
            detected("anna", "Pixel 7a (Anna)"),
            detected("ben", "Pixel 7a (Ben)"),
        ];
        let remembered = vec![
            detected("anna", "stale connected duplicate"),
            detected("old", "Pixel 6"),
        ];

        let projected = project_device_presence(None, &connected, &remembered);

        assert_eq!(
            projected
                .iter()
                .map(|device| device.id.as_str())
                .collect::<Vec<_>>(),
            ["anna", "ben", "old"]
        );
        assert_eq!(projected[0].state, DeviceSessionState::Active);
        assert!(matches!(
            projected[1].state,
            DeviceSessionState::Inert { .. }
        ));
        assert_eq!(projected[2].state, DeviceSessionState::Remembered);
        assert!(!projected[2].state.opens_session());
        assert!(!projected[2].state.offers_sync());
        assert!(!projected[2].state.shows_diff());
        assert_eq!(
            remembered_device_status(None, chrono::Utc::now()),
            "Not connected · never verified"
        );
        let now = chrono::DateTime::from_timestamp(1_753_612_496, 0).unwrap();
        assert_eq!(
            remembered_device_status(Some(now - chrono::Duration::days(3)), now),
            "Not connected · synced 3 days ago"
        );
    }
}
