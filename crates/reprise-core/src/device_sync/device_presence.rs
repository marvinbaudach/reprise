//! Pure projection of connected MTP devices onto Reprise's single session.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetectedDevice {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceSessionState {
    Active,
    Inert { active_device_name: String },
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
    pub fn status_text(&self) -> Option<String> {
        match self {
            Self::Active => None,
            Self::Inert { active_device_name } => Some(format!(
                "Plugged in · disconnect {active_device_name} to use it"
            )),
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
    fn mtp_47_only_the_first_detected_device_owns_the_single_open_session() {
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
}
