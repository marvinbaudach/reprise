//! Wave-one seam for the remembered-device presentation.

use reprise_core::device_sync::DeviceSessionState;

use super::device_sync_page_layout::DeviceDashboard;
use super::device_sync_runtime::DeviceView;

/// Applies the current connected/disconnected decision and reports whether
/// the dashboard remains readable. Plan E replaces this narrow body with the
/// remembered-state projection without reopening the page controller.
pub(super) fn apply(_dashboard: &DeviceDashboard, device: &DeviceView) -> bool {
    page_is_readable(device.connected, &device.session_state)
}

pub(super) fn page_is_readable(connected: bool, session_state: &DeviceSessionState) -> bool {
    connected || *session_state == DeviceSessionState::Remembered
}
