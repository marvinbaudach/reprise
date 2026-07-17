//! Toast delivery for track-list actions.

use super::surface::Shared;

/// Shows `text` as an `adw::Toast`, degrading to a warning when the weak
/// overlay reference has expired or has not been wired yet.
pub(in crate::ui) fn show_toast(shared: &Shared, text: &str) {
    match shared.toast_overlay.upgrade() {
        Some(overlay) => crate::ui::toasts::show(&overlay, text),
        None => tracing::warn!(text, "toast overlay is gone; degrading to log-only"),
    }
}
