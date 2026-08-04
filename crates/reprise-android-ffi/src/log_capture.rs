//! A minimal `tracing::Subscriber` that records each event's fields as plain
//! text, so a test can assert on what a real log line carried without pulling
//! in `tracing-subscriber`'s formatting. Mirrors the capture `reprise-core`'s
//! podcast tests use for the same purpose.
//!
//! Installed per test with `tracing::subscriber::with_default`, which is
//! thread-local — so a test using this one still sees only its own events even
//! though [`crate::init_logging`] has installed a global subscriber for the
//! whole process.

use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub(crate) struct CapturedLogs(Arc<Mutex<Vec<String>>>);

impl CapturedLogs {
    pub(crate) fn joined(&self) -> String {
        self.0.lock().unwrap().join("\n")
    }
}

struct FieldCollector(String);

impl tracing::field::Visit for FieldCollector {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        use std::fmt::Write as _;
        let _ = write!(self.0, " {}={:?}", field.name(), value);
    }
}

pub(crate) struct LogCapture(pub(crate) CapturedLogs);

impl tracing::Subscriber for LogCapture {
    fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }

    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}

    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        let mut collector = FieldCollector(event.metadata().level().to_string());
        event.record(&mut collector);
        self.0 .0.lock().unwrap().push(collector.0);
    }

    fn enter(&self, _span: &tracing::span::Id) {}

    fn exit(&self, _span: &tracing::span::Id) {}
}
