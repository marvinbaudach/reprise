//! Shared tracing capture for yt-dlp boundary tests.

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use super::YtDlpTimeouts;

pub(super) fn fake_binary(directory: &Path, body: &str) -> PathBuf {
    let path = directory.join("fake-yt-dlp");
    fs::write(&path, format!("#!/bin/sh\nset -eu\n{body}\n")).unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&path, permissions).unwrap();
    path
}

pub(super) fn short_timeouts() -> YtDlpTimeouts {
    YtDlpTimeouts {
        version: Duration::from_secs(2),
        update: Duration::from_secs(2),
        list: Duration::from_secs(2),
        search: Duration::from_secs(2),
        resolve: Duration::from_secs(2),
        download: Duration::from_secs(2),
    }
}

#[derive(Clone, Default)]
pub(super) struct CapturedLogs(Arc<Mutex<Vec<String>>>);

impl CapturedLogs {
    pub(super) fn joined(&self) -> String {
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

pub(super) struct LogCapture(pub(super) CapturedLogs);

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
        let mut collector = FieldCollector(event.metadata().name().to_owned());
        event.record(&mut collector);
        self.0 .0.lock().unwrap().push(collector.0);
    }

    fn enter(&self, _span: &tracing::span::Id) {}

    fn exit(&self, _span: &tracing::span::Id) {}
}
