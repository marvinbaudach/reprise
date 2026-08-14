//! A minimal `tracing::Subscriber` that records each event's fields as plain
//! text, so a test can assert on what a real log line carried without pulling
//! in `tracing-subscriber`'s formatting. Mirrors the capture `reprise-core`'s
//! podcast tests use for the same purpose.
//!
//! One process-global subscriber routes events into a thread-local capture.
//! Keeping the dispatcher stable avoids races in tracing's global callsite
//! interest cache when Rust's test runner starts logging tests concurrently.

use std::{
    cell::RefCell,
    sync::{Arc, Mutex},
};

thread_local! {
    static ACTIVE_CAPTURE: RefCell<Option<CapturedLogs>> = const { RefCell::new(None) };
}

#[derive(Clone, Default)]
pub(crate) struct CapturedLogs(Arc<Mutex<Vec<String>>>);

impl CapturedLogs {
    pub(crate) fn joined(&self) -> String {
        self.0.lock().unwrap().join("\n")
    }

    /// Runs one assertion scope with this thread's events routed here.
    pub(crate) fn capture<T>(&self, operation: impl FnOnce() -> T) -> T {
        crate::init_logging();
        let guard = CaptureGuard::install(self.clone());
        let result = operation();
        drop(guard);
        result
    }
}

struct CaptureGuard(Option<CapturedLogs>);

impl CaptureGuard {
    fn install(logs: CapturedLogs) -> Self {
        Self(ACTIVE_CAPTURE.with(|slot| slot.replace(Some(logs))))
    }
}

impl Drop for CaptureGuard {
    fn drop(&mut self) {
        ACTIVE_CAPTURE.with(|slot| {
            slot.replace(self.0.take());
        });
    }
}

struct FieldCollector(String);

impl tracing::field::Visit for FieldCollector {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        use std::fmt::Write as _;
        let _ = write!(self.0, " {}={:?}", field.name(), value);
    }
}

pub(crate) struct CaptureLayer;

impl<S> tracing_subscriber::Layer<S> for CaptureLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _context: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut collector = FieldCollector(event.metadata().level().to_string());
        event.record(&mut collector);
        ACTIVE_CAPTURE.with(|slot| {
            if let Some(logs) = slot.borrow().as_ref() {
                logs.0.lock().unwrap().push(collector.0);
            }
        });
    }
}
