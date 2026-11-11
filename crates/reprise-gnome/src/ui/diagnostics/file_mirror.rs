//! The layer that mirrors log events into the log file.
//!
//! The file leaves the machine inside a Copy Diagnostics report, so its lines
//! pass the same redaction as the session warnings: a structured field is kept
//! only when its key is in `KEEP_FIELDS` (`reprise_core::diagnostics`'s
//! `field_policy`, asked through `is_safe_structured_field` by `EventFields`),
//! and the text goes through `redact_log_message`, which removes library
//! paths, URIs, file names, the home directory and the user name. Every other
//! key is written as `key=$REDACTED`, including a key that is new: adding a
//! field to a log call fails the classification test until its key is listed
//! in `KEEP_FIELDS` or `REDACT_FIELDS`. This layer formats the line itself
//! instead of using `tracing_subscriber`'s formatter, because that formatter
//! prints every field verbatim.
//!
//! The event's own text is the one value the key policy does not see:
//! `EventFields` puts a field called `message` into the message slot, and the
//! text of a log call is a field called `message` too, so the two cannot be
//! told apart. The slot still passes `redact_log_message`, here and for the
//! session warnings at render time, but that pass only removes paths, URIs,
//! file names, the home directory, the user name and unkept `key=` tokens; a
//! GStreamer string with a host or a title survives it. The classification
//! test therefore refuses an explicit `message` field in production code.

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

    use reprise_core::diagnostics::{render_report, DiagnosticFacts, DiagnosticLog};
    use reprise_core::up_next::QueueItem;
    use reprise_view::queue::rows::QueueRow;
    use tracing_subscriber::layer::SubscriberExt;

    use super::*;

    #[derive(Debug)]
    enum Playback {
        Playing,
    }

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

    #[test]
    fn kept_fields_arrive_verbatim_and_the_error_is_redacted_in_the_file_and_the_copied_report() {
        let text = mirrored(|| {
            tracing::warn!(
                elapsed_ms = 12,
                placeholder_distance = 0.25,
                version = "0.1.331",
                volume = 0.5,
                failure_message = "Artist - Title could not be decoded",
                state = ?Playback::Playing,
                count = 3,
                error = "could not open /home/someone/Music/Artist/x.flac",
                "playback stalled"
            );
        });
        // The report re-redacts the file's lines when it embeds the tail.
        let facts = DiagnosticFacts {
            log_tail: Some(text.clone()),
            ..DiagnosticFacts::default()
        };
        let report = render_report(&facts, &DiagnosticLog::default(), &context());

        // Every line starts with an RFC 3339 time whose `.123Z` looks like a
        // file extension; the report's second redaction must leave it alone.
        let timestamp = text.split(' ').next().unwrap_or_default();
        assert!(
            timestamp.len() == "2026-11-08T12:00:00.123Z".len() && timestamp.ends_with('Z'),
            "unexpected line start: {text}"
        );
        for (surface, shown) in [("the log file", &text), ("the copied report", &report)] {
            assert!(
                shown.contains(&format!("{timestamp}  WARN ")),
                "{surface} lost the line's timestamp {timestamp:?}: {shown}"
            );
            for kept in [
                "elapsed_ms=12",
                "placeholder_distance=0.25",
                "version=0.1.331",
                "state=Playing",
                "count=3",
                "error=$REDACTED",
                // A redacted key stays redacted whatever its value looks like.
                "volume=$REDACTED",
                "failure_message=$REDACTED",
            ] {
                assert!(shown.contains(kept), "{surface} lost {kept:?}: {shown}");
            }
            for leaked in [
                "x.flac",
                "Artist",
                "/home/someone",
                "someone",
                "decoded",
                "volume=0.5",
            ] {
                assert!(
                    !shown.contains(leaked),
                    "{surface} leaked {leaked:?}: {shown}"
                );
            }
        }
    }

    #[test]
    fn a_kept_composite_field_arrives_verbatim_and_a_foreign_one_does_not() {
        let text = mirrored(|| {
            tracing::info!(
                current_up_next = ?Some(QueueItem::Track(5)),
                episode_ids = ?vec![1_i64, 2, 3],
                rows = ?[QueueRow::NowPlaying, QueueRow::PlayNext(0), QueueRow::UpNext(1)],
                state = ?Some("Pink Floyd"),
                "queue changed"
            );
        });
        let facts = DiagnosticFacts {
            log_tail: Some(text.clone()),
            ..DiagnosticFacts::default()
        };
        let report = render_report(&facts, &DiagnosticLog::default(), &context());

        for (surface, shown) in [("the log file", &text), ("the copied report", &report)] {
            for kept in [
                "current_up_next=Some(Track(5))",
                "episode_ids=[1, 2, 3]",
                "rows=[NowPlaying, PlayNext(0), UpNext(1)]",
                "state=$REDACTED",
            ] {
                assert!(shown.contains(kept), "{surface} lost {kept:?}: {shown}");
            }
            assert!(!shown.contains("Pink"), "{surface} leaked a name: {shown}");
        }
    }
}
