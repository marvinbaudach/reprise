//! Issue detection via lofty tracing-warning capture.
//!
//! Temporarily installs a minimal [`tracing::Subscriber`] on the calling
//! thread that collects WARN-level events emitted by lofty during
//! [`lofty::read_from_path`].  The captured messages are then classified
//! into [`Issue`] variants.

use std::path::Path;
use std::sync::{Arc, Mutex};

use super::{Diagnosis, Issue, RepairError};

/// A single warning captured from lofty's tracing output.
#[derive(Debug, Clone)]
pub struct CapturedWarning {
    pub target: String,
    pub message: String,
}

// ── tracing subscriber ───────────────────────────────────────────────

/// Minimal subscriber that only captures WARN events whose target starts
/// with `"lofty"`.
struct LoftyWarningCollector {
    sink: Arc<Mutex<Vec<CapturedWarning>>>,
}

impl tracing::Subscriber for LoftyWarningCollector {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        *metadata.level() == tracing::Level::WARN && metadata.target().starts_with("lofty")
    }

    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        let target = event.metadata().target().to_owned();
        let mut visitor = MessageVisitor(String::new());
        event.record(&mut visitor);
        self.sink.lock().unwrap().push(CapturedWarning {
            target,
            message: visitor.0,
        });
    }
}

/// Extracts the `message` field from a tracing event.
struct MessageVisitor(String);

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{value:?}");
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.0 = value.to_owned();
        }
    }
}

// ── classification ───────────────────────────────────────────────────

/// Classify captured lofty warnings into deduplicated [`Issue`] variants.
pub fn classify_warnings(warnings: &[CapturedWarning]) -> Vec<Issue> {
    let mut issues = Vec::new();

    let has = |pred: &dyn Fn(&CapturedWarning) -> bool| warnings.iter().any(pred);

    if has(&|w| w.message.contains("ilst") && w.message.contains("Multiple")) {
        issues.push(Issue::DuplicateIlst);
    }
    if has(&|w| {
        w.message.contains("Failed to read frame header")
            || w.message.contains("Failed to parse a frame ID")
    }) {
        issues.push(Issue::CorruptId3Frames);
    }
    if has(&|w| w.message.contains("Using bitrate to estimate duration")) {
        issues.push(Issue::MissingVbrHeader);
    }

    issues
}

// ── public API ───────────────────────────────────────────────────────

/// Diagnose metadata issues in a single audio file.
///
/// Temporarily replaces the tracing subscriber on the calling thread to
/// capture lofty warnings.  The file is read but **not** modified.
pub fn diagnose(path: &Path) -> Result<Diagnosis, RepairError> {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let collector = LoftyWarningCollector { sink: sink.clone() };
    let dispatch = tracing::dispatcher::Dispatch::new(collector);

    // Temporarily install our collector for this thread only.
    let _result = tracing::dispatcher::with_default(&dispatch, || {
        lofty::read_from_path(path)
    })?;

    let warnings = sink.lock().unwrap();
    let issues = classify_warnings(&warnings);

    Ok(Diagnosis {
        path: path.to_path_buf(),
        issues,
    })
}

#[cfg(test)]
#[path = "diagnosis_tests.rs"]
mod tests;
