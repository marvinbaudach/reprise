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
            device_sync_strings::text(device_sync_strings::CONTENTS_NEVER_VERIFIED),
            device_sync_strings::text(device_sync_strings::CONTENTS_SCAN_INVITATION),
            true,
        ),
        DeviceContentsState::Verifying => (
            device_sync_strings::text(device_sync_strings::CONTENTS_VERIFYING),
            device_sync_strings::text(device_sync_strings::CONTENTS_VERIFYING_DETAIL),
            false,
        ),
        DeviceContentsState::Verified => {
            let title = last_verified_at.map_or_else(
                || device_sync_strings::text(device_sync_strings::CONTENTS_VERIFIED),
                |verified_at| device_sync_strings::verified_ago(now, verified_at),
            );
            (title, String::new(), true)
        }
        DeviceContentsState::VerifiedEarlier(verified_at) => (
            device_sync_strings::verified_ago(now, *verified_at),
            String::new(),
            false,
        ),
        DeviceContentsState::Failed(error) => (
            device_sync_strings::text(device_sync_strings::CONTENTS_NOT_VERIFIABLE),
            error.clone(),
            true,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mtp_26_verification_copy_names_all_five_states_and_gates_the_scan_action() {
        let now = chrono::DateTime::from_timestamp(1_785_183_239, 0).unwrap();
        let (title, _, can_scan) =
            verification_copy(&DeviceContentsState::NeverVerified, None, now);
        assert_eq!(title, "Device contents never verified");
        assert!(can_scan);

        let (_, _, can_scan) = verification_copy(&DeviceContentsState::Verifying, None, now);
        assert!(!can_scan);

        let verified_earlier = now - chrono::Duration::days(1);
        let (title, detail, can_scan) = verification_copy(
            &DeviceContentsState::VerifiedEarlier(verified_earlier),
            Some(verified_earlier),
            now,
        );
        assert_eq!(title, "verified 1 d ago");
        assert!(detail.is_empty());
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
