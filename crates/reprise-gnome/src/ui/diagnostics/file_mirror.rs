//! The layer that mirrors log events into the run log.
//!
//! The file leaves the machine inside a Copy Diagnostics report, so a line is
//! written only as `reprise_core::diagnostics::SafeEvent::line()` makes it:
//! the call site's static template instead of the rendered message, a kept
//! field's typed number, `key=$REDACTED` for every other key, and ` @ <file>:<line>`
//! naming the call site. An event from a dependency (a call site the template
//! table does not know, tracing-log's bridged records included) is its level,
//! its target and `…`. Nothing is searched for in the text: what is written is
//! static by construction, so no redaction context exists any more.
//!
//! This layer formats the line itself instead of using `tracing_subscriber`'s
//! formatter, because that formatter prints every field and the message
//! verbatim. Adding a field to a log call fails the classification test until
//! its key is listed in `KEEP_FIELDS` or `REDACT_FIELDS`, and an explicit
//! `message` field in production code is refused there too.

use std::io::Write as _;

use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};

use super::event_record::safe_of;
use super::file_log::FileLogWriter;

/// Mirrors events into `sink`, one safe line per event.
pub(crate) struct FileMirrorLayer<W> {
    sink: W,
}

impl FileMirrorLayer<fn(&str)> {
    /// The mirror of the running application, writing to the shared run log.
    pub(crate) fn for_this_machine() -> Self {
        Self::new(write_to_file_log)
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
    fn new(sink: W) -> Self {
        Self { sink }
    }
}

impl<S, W> Layer<S> for FileMirrorLayer<W>
where
    S: Subscriber,
    W: Fn(&str) + 'static,
{
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        (self.sink)(&safe_of(event).line());
    }
}

#[cfg(test)]
mod tests;
