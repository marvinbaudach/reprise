//! Test-only capture of what GTK and libadwaita log.
//!
//! GTK logs through `g_log_structured`, which GLib's per-domain handlers
//! (`glib::log_set_handler`) never see; only the process-wide writer does.
//! That writer can be installed once per process, so installation is guarded
//! and every test that needs the log shares it. The default output is kept.

use std::sync::{Mutex, Once};

static INSTALL: Once = Once::new();
static MESSAGES: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Starts capturing and returns a mark for [`messages_since`].
pub(crate) fn mark() -> usize {
    INSTALL.call_once(|| {
        gtk4::glib::log_set_writer_func(|_, fields| {
            let message = fields
                .iter()
                .find(|field| field.key() == "MESSAGE")
                .and_then(|field| field.value_str());
            if let (Some(message), Ok(mut captured)) = (message, MESSAGES.lock()) {
                captured.push(message.to_string());
            }
            gtk4::glib::LogWriterOutput::Unhandled
        });
    });
    MESSAGES.lock().map_or(0, |captured| captured.len())
}

/// Every message logged since `mark`.
pub(crate) fn messages_since(mark: usize) -> Vec<String> {
    MESSAGES
        .lock()
        .map(|captured| captured.iter().skip(mark).cloned().collect())
        .unwrap_or_default()
}
