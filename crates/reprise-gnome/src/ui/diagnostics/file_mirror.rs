//! The layer that mirrors log events into the log file.
//!
//! The file leaves the machine inside a Copy Diagnostics report, so its lines
//! pass the same redaction as the session warnings: a structured field is kept
//! only when `is_safe_structured_field` says so (`EventFields`), and the text
//! goes through `redact_log_message`, which removes library paths, URIs, file
//! names, the home directory and the user name. This layer formats the line
//! itself instead of using `tracing_subscriber`'s formatter, because that
//! formatter prints every field verbatim.

use std::io::Write as _;

use chrono::{SecondsFormat, Utc};
use reprise_core::diagnostics::{redact_log_message, RedactionContext};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};

use super::file_log::FileLogWriter;
use super::EventFields;

/// Mirrors events into `sink`, one redacted line per event.
pub(crate) struct FileMirrorLayer<W> {
    context: RedactionContext,
    sink: W,
}

impl FileMirrorLayer<fn(&str)> {
    /// The mirror of the running application: this machine's redaction
    /// context, writing to the shared log file.
    pub(crate) fn for_this_machine() -> Self {
        Self::new(
            reprise_platform_linux::diagnostics::redaction_context(),
            write_to_file_log,
        )
    }
}

fn write_to_file_log(line: &str) {
    // A full or broken log file must never disturb the application.
    let _ = FileLogWriter::new().write_all(line.as_bytes());
}

impl<W> FileMirrorLayer<W>
where
    W: Fn(&str),
{
    fn new(context: RedactionContext, sink: W) -> Self {
        Self { context, sink }
    }
}

impl<S, W> Layer<S> for FileMirrorLayer<W>
where
    S: Subscriber,
    W: Fn(&str) + 'static,
{
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        let metadata = event.metadata();
        let mut fields = EventFields::default();
        event.record(&mut fields);
        let text = redact_log_message(&fields.into_message(), &self.context);
        let target = redact_log_message(metadata.target(), &self.context);
        let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        (self.sink)(&format!(
            "{timestamp} {:>5} {target}: {text}\n",
            metadata.level()
        ));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use tracing_subscriber::layer::SubscriberExt;

    use super::*;

    fn context() -> RedactionContext {
        RedactionContext {
            music_dir: Some("/home/someone/Music".to_owned()),
            home_dir: Some("/home/someone".to_owned()),
            username: Some("someone".to_owned()),
        }
    }

    fn mirrored(log: impl FnOnce()) -> String {
        let lines = Arc::new(Mutex::new(String::new()));
        let sink = lines.clone();
        let layer = FileMirrorLayer::new(context(), move |line: &str| {
            sink.lock().unwrap().push_str(line);
        });
        tracing::subscriber::with_default(tracing_subscriber::registry().with(layer), log);
        let text = lines.lock().unwrap().clone();
        text
    }

    #[test]
    fn a_library_path_in_the_message_or_a_field_never_reaches_the_file() {
        let text = mirrored(|| {
            tracing::warn!(
                path = "/home/someone/Music/Artist/x.flac",
                track_id = 7,
                "could not read /home/someone/Music/Artist/x.flac for someone"
            );
        });

        assert!(text.contains("track_id=7"), "{text}");
        for leaked in ["x.flac", "Artist", "/home/someone", "someone"] {
            assert!(
                !text.contains(leaked),
                "{leaked:?} reached the file: {text}"
            );
        }
        assert!(text.contains("path=$REDACTED"), "{text}");
    }

    #[test]
    fn a_line_names_its_time_level_and_target() {
        let text = mirrored(|| tracing::info!("starting Reprise"));

        assert!(text.ends_with("starting Reprise\n"), "{text}");
        assert!(text.contains(" INFO "), "{text}");
        assert!(text.contains("reprise"), "{text}");
        assert!(text.starts_with("20"), "{text}");
    }
}
