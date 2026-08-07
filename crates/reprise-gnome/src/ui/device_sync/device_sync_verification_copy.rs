use reprise_core::device_sync::device_view::DeviceContentsState;

use super::device_sync_strings;

/// `MTP-26`: the verification status copy and whether "Rescan" is enabled.
/// Pure so the exact copy remains unit-tested without a display.
pub(super) fn verification_copy(
    state: &DeviceContentsState,
    last_verified_at: Option<chrono::DateTime<chrono::Utc>>,
    now: chrono::DateTime<chrono::Utc>,
) -> (String, String, bool) {
    match state {
        DeviceContentsState::NeverVerified => (
            "Device contents never verified".to_string(),
            "Scan the device to see what's already there before syncing.".to_string(),
            true,
        ),
        DeviceContentsState::Verifying => (
            "Verifying device contents…".to_string(),
            "Reading storage over MTP — this can take a moment.".to_string(),
            false,
        ),
        DeviceContentsState::Verified => {
            let title = last_verified_at.map_or_else(
                || "Device contents verified".to_string(),
                |verified_at| device_sync_strings::verified_ago(now, verified_at),
            );
            (title, String::new(), true)
        }
        DeviceContentsState::Failed(error) => (
            "Could not verify device contents".to_string(),
            error.clone(),
            true,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mtp_26_verification_copy_names_all_four_states_and_gates_the_scan_action() {
        let now = chrono::DateTime::from_timestamp(1_785_183_239, 0).unwrap();
        let (title, _, can_scan) =
            verification_copy(&DeviceContentsState::NeverVerified, None, now);
        assert_eq!(title, "Device contents never verified");
        assert!(can_scan);

        let (_, _, can_scan) = verification_copy(&DeviceContentsState::Verifying, None, now);
        assert!(!can_scan);

        let (title, detail, can_scan) = verification_copy(
            &DeviceContentsState::Verified,
            Some(now - chrono::Duration::minutes(2)),
            now,
        );
        assert_eq!(title, "verified 2 min ago");
        assert!(detail.is_empty());
        assert!(can_scan);

        let (title, _, can_scan) = verification_copy(&DeviceContentsState::Verified, None, now);
        assert_eq!(title, "Device contents verified");
        assert!(can_scan);

        let (title, detail, can_scan) = verification_copy(
            &DeviceContentsState::Failed("MTP timeout".into()),
            None,
            now,
        );
        assert_eq!(title, "Could not verify device contents");
        assert_eq!(detail, "MTP timeout");
        assert!(can_scan, "a failed scan must still offer retry");
    }
}
