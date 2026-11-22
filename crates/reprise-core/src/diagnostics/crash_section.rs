//! The crash report inside a log tail.
//!
//! The report is a section of fixed lines the panic hook writes: a time, a
//! version, a source location, the panic message and a backtrace. Run through
//! the free-text passes, its `message:` key and every `at ./crates/…rs:120:9`
//! line would read as a path or a field and be replaced, and the section would
//! no longer say where the program crashed. So the section is read as what it
//! is:
//!
//! - a source location (`crates/…/x.rs:120:9`, `src/main.rs:12:5`, a
//!   toolchain's `/rustc/<hash>/library/…`) is built from characters no user
//!   data needs, and stays as written;
//! - the panic message is free text and goes through the redactor, key
//!   stripped, so a path in it takes the rest of the line;
//! - every other line goes through the redactor as it is.
//!
//! A line that only looks like a location (a space in it, a user's folder, a
//! name that is not in the repository layout) is not one.

use crate::crash_report::{CRASH_REPORT_END, CRASH_REPORT_MARKER};

use super::model::RedactionContext;
use super::redact::{redact_log_message, redact_names};

/// The folders a source path of this repository starts with.
const SOURCE_ROOTS: [&str; 5] = ["crates", "src", "tests", "benches", "examples"];

const LOCATION_KEY: &str = "location:";
const MESSAGE_KEY: &str = "message:";
const FRAME_PREFIX: &str = "at ";

/// Where the redaction of a log tail stands: inside a crash report or not.
#[derive(Default)]
pub(super) struct TailState {
    in_crash_report: bool,
}

impl TailState {
    /// The line as the report may carry it.
    pub(super) fn redact_line(&mut self, line: &str, context: &RedactionContext) -> String {
        if line == CRASH_REPORT_MARKER {
            self.in_crash_report = true;
            return line.to_owned();
        }
        if !self.in_crash_report {
            return redact_log_message(line, context);
        }
        if line == CRASH_REPORT_END {
            self.in_crash_report = false;
            return line.to_owned();
        }
        redact_crash_line(line, context)
    }
}

fn redact_crash_line(line: &str, context: &RedactionContext) -> String {
    if let Some(location) = line.strip_prefix(LOCATION_KEY) {
        if is_source_location(location.trim()) {
            return redact_names(line, context);
        }
    } else if let Some(message) = line.strip_prefix(MESSAGE_KEY) {
        let message = message.strip_prefix(' ').unwrap_or(message);
        return format!("{MESSAGE_KEY} {}", redact_log_message(message, context));
    } else if let Some(location) = line.trim_start().strip_prefix(FRAME_PREFIX) {
        if is_source_location(location) {
            return redact_names(line, context);
        }
    }
    redact_log_message(line, context)
}

/// `crates/reprise-gnome/src/ui/window/window.rs:120:9`, `./src/main.rs:12:5`,
/// `/rustc/abcdef0123/library/core/src/panicking.rs:280:5`: a path of plain
/// segments to a Rust file, then a line and a column.
fn is_source_location(text: &str) -> bool {
    let Some((path, position)) = text.split_once(".rs:") else {
        return false;
    };
    let mut numbers = position.split(':');
    let numeric = |part: Option<&str>| {
        part.is_some_and(|digits| !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()))
    };
    if !(numeric(numbers.next()) && numeric(numbers.next()) && numbers.next().is_none()) {
        return false;
    }
    let (rooted, relative) = match path.strip_prefix('/') {
        Some(absolute) => (true, absolute),
        None => (false, path.strip_prefix("./").unwrap_or(path)),
    };
    let segments: Vec<&str> = relative.split('/').collect();
    let plain = segments.iter().all(|segment| {
        !segment.is_empty()
            && !matches!(*segment, "." | "..")
            && segment
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '+'))
    });
    plain
        && if rooted {
            is_toolchain_source(&segments)
        } else {
            starts_in_repository(&segments)
        }
}

fn starts_in_repository(segments: &[&str]) -> bool {
    segments.len() >= 2
        && segments
            .first()
            .is_some_and(|root| SOURCE_ROOTS.contains(root))
}

/// `/rustc/<hash>/library/…`: the standard library's own source.
fn is_toolchain_source(segments: &[&str]) -> bool {
    const MIN_HASH_CHARS: usize = 8;
    segments.len() >= 3
        && segments.first() == Some(&"rustc")
        && segments.get(1).is_some_and(|hash| {
            hash.len() >= MIN_HASH_CHARS && hash.chars().all(|c| c.is_ascii_hexdigit())
        })
        && segments.get(2) == Some(&"library")
}
