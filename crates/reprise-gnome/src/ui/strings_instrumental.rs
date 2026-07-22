//! Labels for the experimental instrumental-fassungen surface (docs/ux-rules
//! Section AB). This file grows as each surface (context menu, AI badge,
//! preferences) lands; today it carries the conversion/staging view.

use super::text;

// Conversion/staging view (INST-2..8).
pub const CONVERSION_TITLE: &str = N_!("Instrumental conversions");
pub const CONVERSION_EMPTY: &str = N_!("No conversions yet");
pub const CONVERSION_SAVE_ALL: &str = N_!("Save all");
pub const CONVERSION_CLEAR: &str = N_!("Clear playlist");
pub const CONVERSION_SAVE: &str = N_!("Save");
pub const CONVERSION_DISCARD: &str = N_!("Discard");
pub const CONVERSION_PLAY: &str = N_!("Play");

pub const STATE_QUEUED: &str = N_!("Queued");
pub const STATE_PROCESSING: &str = N_!("Processing…");
pub const STATE_READY_UNSAVED: &str = N_!("Ready — not saved");
pub const STATE_SAVED: &str = N_!("Saved to library");
pub const STATE_FAILED: &str = N_!("Failed");

/// The conversion header's aggregate figure, e.g. "3 of 8 · 38%" (INST-2).
pub fn conversion_aggregate(done: usize, total: usize, percent: u16) -> String {
    format!("{done} of {total} · {percent}%")
}

// "Hide AI music" library filter (FIL-7).
pub const FILTER_HIDE_AI: &str = N_!("Hide AI music");

/// The accessible label for the AI-filter chip's remove (×) affordance.
pub fn remove_hide_ai_filter() -> String {
    format!("Remove filter: {}", text(FILTER_HIDE_AI))
}

// Experimental preferences page (INST-11/INST-12).
pub const EXPERIMENTAL_PAGE_TITLE: &str = N_!("Experimental");
pub const EXPERIMENTAL_GROUP_TITLE: &str = N_!("Experimental features");
pub const EXPERIMENTAL_GROUP_DESCRIPTION: &str =
    N_!("Unfinished features with rough edges, off by default.");
pub const EXPERIMENTAL_TOGGLE_TITLE: &str = N_!("Enable experimental features");
pub const EXPERIMENTAL_TOGGLE_SUBTITLE: &str =
    N_!("Shows AI instrumental versions across the app: the context-menu trigger, the conversion view, badges, and the \"Hide AI music\" filter.");
pub const MODEL_GROUP_TITLE: &str = N_!("Instrumental model");
pub const MODEL_DOWNLOAD_TITLE: &str = N_!("Vocal-removal model");
pub const MODEL_DOWNLOAD_SUBTITLE: &str = N_!(
    "Downloaded on first use (~316 MB), verified by checksum, with a licence note kept beside it."
);
pub const MODEL_DOWNLOAD_BUTTON: &str = N_!("Download");

/// INST-12: shown in a build compiled without the `stem-backend` feature — an
/// honest placeholder, never a functionless enabled button.
pub const MODEL_UNAVAILABLE_SUBTITLE: &str =
    N_!("This build has no stem-separation backend, so the model can't be downloaded here.");
/// INST-12: the model is present and verified; instrumental rendering works.
pub const MODEL_READY_SUBTITLE: &str = N_!("Model ready — instrumental rendering is available.");
/// INST-12: the render is being verified/published after the bytes arrive. Only
/// the `stem-backend` build reaches the download flow that shows it.
#[cfg(feature = "stem-backend")]
pub const MODEL_FINISHING: &str = N_!("Verifying…");

/// INST-12 progress line while the weights download, e.g. "Downloading… 42%".
/// Only the `stem-backend` build runs the download that shows it.
#[cfg(feature = "stem-backend")]
pub fn model_downloading(percent: u16) -> String {
    format!("Downloading… {percent}%")
}

/// INST-12 indeterminate progress line when the server declares no size.
#[cfg(feature = "stem-backend")]
pub fn model_downloading_indeterminate() -> String {
    "Downloading…".to_string()
}

/// INST-12 failure line, e.g. offline or a checksum mismatch. The detail is the
/// core `ProvisionError` text.
#[cfg(feature = "stem-backend")]
pub fn model_download_failed(detail: &str) -> String {
    format!("Download failed: {detail}")
}
