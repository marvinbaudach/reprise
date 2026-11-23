//! The one producer of log text for the report and the run log.
//!
//! [`safe_event`] turns a recorded tracing event into a [`SafeEvent`] whose
//! text is static by construction. The rendered message of the event is never
//! copied into the output: it only selects which static template of the call
//! site's source file stands in for it (`log_templates::reduced_by`), and
//! no match gives `…`. A field is shown by `field_values` (a kept key's typed
//! number, or a kept key's text of plain shape) and is `key=$REDACTED`
//! otherwise. A call site whose file the template table does not know, which
//! is a dependency's or a bridged `log` record, shows its level and target and
//! `…`: its fields and message are not read at all.
//!
//! The types here are plain data. A layer records the event into
//! [`RecordedEvent`] and [`Callsite`] and asks for the line; `reprise-core`
//! needs nothing of tracing for that.
//!
//! A line from own code ends with ` @ <file>:<line>`, the call site's static
//! location, so a reader can find the code that logged it; a foreign line has
//! no location. A line feed in a template is written as the two characters
//! `\n`, so one event is one line.

use chrono::{DateTime, SecondsFormat, Utc};

use super::field_values::{render_field, RecordedField};
use super::log_template::{LogTemplate, HOLE};
use super::log_templates::{is_own_file, reduced_by, templates_for_file};

/// What an event with neither a message nor a field shows.
const NO_DETAILS: &str = "warning without details";

/// The level of an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    /// The level as the log writes it, without padding.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

/// The static metadata of a call site (tracing's `Metadata`, all `'static`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Callsite {
    /// `Metadata::file()`: the path of the source file as `file!()` gave it.
    pub file: Option<&'static str>,
    pub line: Option<u32>,
    pub target: &'static str,
    pub level: LogLevel,
}

/// What a layer read out of an event's values.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecordedEvent {
    /// The rendered text of the event's `message` field.
    pub message: Option<String>,
    /// Every other field, in the order they were recorded.
    pub fields: Vec<RecordedField>,
}

/// An event reduced to text that may leave the machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SafeEvent {
    level: LogLevel,
    target: &'static str,
    body: String,
    location: Option<String>,
}

/// The safe form of one event. `callsite.file` decides between an own event,
/// whose message is its static template and whose kept fields are shown, and a
/// foreign one, which is its level, target and `…`.
pub fn safe_event(callsite: &Callsite, recorded: &RecordedEvent) -> SafeEvent {
    match callsite.file.filter(|file| is_own_file(file)) {
        Some(file) => own_event(callsite, file, templates_for_file(file), recorded),
        None => foreign_event(callsite),
    }
}

/// An event from own code against `templates`, the templates of `file`.
///
/// The message only picks a template (`reduced_by`); the text written is the
/// template's. Fields go through `render_field`.
pub(super) fn own_event(
    callsite: &Callsite,
    file: &'static str,
    templates: &[LogTemplate],
    recorded: &RecordedEvent,
) -> SafeEvent {
    let message = recorded
        .message
        .as_deref()
        .map(|rendered| reduced_by(templates, rendered));
    let fields = recorded
        .fields
        .iter()
        .map(|field| render_field(field.key, &field.value))
        .collect::<Vec<_>>()
        .join(" ");
    let body = match (message, fields.is_empty()) {
        (Some(message), true) => message,
        (Some(message), false) => format!("{message}; {fields}"),
        (None, false) => fields,
        (None, true) => NO_DETAILS.to_owned(),
    };
    SafeEvent {
        level: callsite.level,
        target: callsite.target,
        body: one_line(&body),
        location: Some(location(file, callsite.line)),
    }
}

/// An event that is not from own code: nothing of it but level and target.
pub(super) fn foreign_event(callsite: &Callsite) -> SafeEvent {
    SafeEvent {
        level: callsite.level,
        target: callsite.target,
        body: HOLE.to_owned(),
        location: None,
    }
}

/// The text with each line break written as two characters, so that an event
/// stays one line. The one implementation: the panic reducer and the tests use it
/// too.
pub(super) fn one_line(text: &str) -> String {
    text.replace('\n', "\\n").replace('\r', "\\r")
}

fn location(file: &str, line: Option<u32>) -> String {
    line.map_or_else(|| file.to_owned(), |line| format!("{file}:{line}"))
}

impl SafeEvent {
    pub fn level(&self) -> LogLevel {
        self.level
    }

    pub fn target(&self) -> &'static str {
        self.target
    }

    /// The final `::` segment of the target; the whole target when it has none.
    pub fn target_segment(&self) -> &'static str {
        self.target
            .rsplit_once("::")
            .map_or(self.target, |(_, segment)| segment)
    }

    /// The message, then `; ` and the fields. One line, never empty.
    pub fn body(&self) -> &str {
        &self.body
    }

    /// `<file>:<line>` of an own call site; `None` for a foreign one.
    pub fn location(&self) -> Option<&str> {
        self.location.as_deref()
    }

    /// ` @ <file>:<line>`, or nothing for a foreign event.
    pub fn suffix(&self) -> String {
        self.location
            .as_deref()
            .map_or_else(String::new, |location| format!(" @ {location}"))
    }

    /// The run-log line for an event that happened at `at`, with its line
    /// terminator: `<rfc3339 ms>Z {level:>5} <target>: <body><suffix>\n`.
    pub fn line_at(&self, at: DateTime<Utc>) -> String {
        format!(
            "{} {:>5} {}: {}{}\n",
            at.to_rfc3339_opts(SecondsFormat::Millis, true),
            self.level.as_str(),
            self.target,
            self.body,
            self.suffix()
        )
    }

    /// [`Self::line_at`] for now, on our own clock.
    pub fn line(&self) -> String {
        self.line_at(Utc::now())
    }
}
