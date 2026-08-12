//! Fallible, read-only runtime facts owned by the Linux multimedia boundary.

use gstreamer as gst;

pub fn gstreamer_version() -> Option<String> {
    gst::init().ok()?;
    let (major, minor, micro, _) = gst::version();
    Some(format!("{major}.{minor}.{micro}"))
}

/// Reprise can name the selected sink only when its explicit override is in use.
/// GStreamer's automatic sink is resolved inside playbin and is otherwise left
/// unknown rather than guessed from the factories installed on the host.
pub fn active_audio_backend() -> Option<String> {
    std::env::var("REPRISE_AUDIO_SINK")
        .ok()
        .filter(|value| !value.is_empty())
}
