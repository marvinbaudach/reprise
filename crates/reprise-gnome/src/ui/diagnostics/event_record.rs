//! Reads a tracing event into what `reprise_core::diagnostics::safe_event`
//! turns into a safe line.
//!
//! One visitor records each field with the kind the call site gave it, so the
//! core can keep a typed number and refuse a text (`RecordedValue`). The field
//! called `message` fills the message slot: it is the text of the log call, and
//! the core never copies it, it only picks the call site's template with it.

use std::fmt::Debug;

use reprise_core::diagnostics::{
    is_own_file, safe_event, Callsite, LogLevel, RecordedEvent, RecordedField, RecordedValue,
    SafeEvent,
};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Metadata};

/// The only field whose value is the event's message.
const MESSAGE_FIELD: &str = "message";

/// The safe form of `event`. A call site the template table does not know is
/// a dependency's: its values are not even read.
pub(super) fn safe_of(event: &Event<'_>) -> SafeEvent {
    let metadata = event.metadata();
    let callsite = callsite_of(metadata);
    let recorded = if callsite.file.is_some_and(is_own_file) {
        recorded_of(event)
    } else {
        RecordedEvent::default()
    };
    safe_event(&callsite, &recorded)
}

/// The static facts of the call site. `Event::metadata` is `'static`, and so
/// are its file and target.
pub(super) fn callsite_of(metadata: &'static Metadata<'static>) -> Callsite {
    Callsite {
        file: metadata.file(),
        line: metadata.line(),
        target: metadata.target(),
        level: level_of(*metadata.level()),
    }
}

fn level_of(level: Level) -> LogLevel {
    match level {
        Level::TRACE => LogLevel::Trace,
        Level::DEBUG => LogLevel::Debug,
        Level::INFO => LogLevel::Info,
        Level::WARN => LogLevel::Warn,
        Level::ERROR => LogLevel::Error,
    }
}

/// Every value of `event`, by kind.
pub(super) fn recorded_of(event: &Event<'_>) -> RecordedEvent {
    let mut visitor = KindVisitor::default();
    event.record(&mut visitor);
    visitor.recorded
}

#[derive(Default)]
struct KindVisitor {
    recorded: RecordedEvent,
}

impl KindVisitor {
    fn push(&mut self, field: &Field, value: RecordedValue) {
        if field.name() == MESSAGE_FIELD {
            self.recorded.message = Some(text_of(&value));
        } else {
            self.recorded
                .fields
                .push(RecordedField::new(field.name(), value));
        }
    }
}

/// The text a `message` value renders to, whatever kind it was recorded as.
fn text_of(value: &RecordedValue) -> String {
    match value {
        RecordedValue::Int(number) => number.to_string(),
        RecordedValue::Uint(number) => number.to_string(),
        RecordedValue::Float(number) => number.to_string(),
        RecordedValue::Bool(flag) => flag.to_string(),
        RecordedValue::Str(text) | RecordedValue::Debug(text) => text.clone(),
    }
}

impl Visit for KindVisitor {
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.push(field, RecordedValue::Int(i128::from(value)));
    }

    fn record_i128(&mut self, field: &Field, value: i128) {
        self.push(field, RecordedValue::Int(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.push(field, RecordedValue::Uint(u128::from(value)));
    }

    fn record_u128(&mut self, field: &Field, value: u128) {
        self.push(field, RecordedValue::Uint(value));
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.push(field, RecordedValue::Float(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.push(field, RecordedValue::Bool(value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.push(field, RecordedValue::Str(value.to_owned()));
    }

    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.push(field, RecordedValue::Debug(format!("{value:?}")));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        self.push(field, RecordedValue::Debug(format!("{value:?}")));
    }
}

#[cfg(test)]
mod tests;
