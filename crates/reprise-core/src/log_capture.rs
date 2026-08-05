//! A minimal `tracing::Subscriber` that records each event's name and fields as
//! plain text, so a test can assert on what a real log line carried without
//! pulling in `tracing-subscriber`'s formatting.
//!
//! One process-global subscriber routes events into a thread-local capture.
//! Keeping the dispatcher stable avoids races in tracing's global callsite
//! interest cache when Rust's test runner starts logging tests concurrently.

use std::{
    cell::RefCell,
    sync::{Arc, Mutex, OnceLock},
};

static INSTALL_CAPTURE: OnceLock<()> = OnceLock::new();

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
        INSTALL_CAPTURE.get_or_init(|| {
            tracing::subscriber::set_global_default(LogCapture)
                .expect("test log capture must own the Core test subscriber");
        });
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

struct LogCapture;

impl tracing::Subscriber for LogCapture {
    fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
        ACTIVE_CAPTURE.with(|slot| slot.borrow().is_some())
    }

    fn register_callsite(
        &self,
        _metadata: &'static tracing::Metadata<'static>,
    ) -> tracing::subscriber::Interest {
        tracing::subscriber::Interest::sometimes()
    }

    fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }

    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}

    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        let mut collector = FieldCollector(event.metadata().name().to_owned());
        event.record(&mut collector);
        ACTIVE_CAPTURE.with(|slot| {
            if let Some(logs) = slot.borrow().as_ref() {
                logs.0.lock().unwrap().push(collector.0);
            }
        });
    }

    fn enter(&self, _span: &tracing::span::Id) {}

    fn exit(&self, _span: &tracing::span::Id) {}
}
