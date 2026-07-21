//! Shared output helpers.
//!
//! `--json` selects machine-readable output; without it commands print a
//! compact human-readable rendering. JSON always goes to stdout as a single
//! pretty-printed document so a caller can pipe it straight into a parser.

use serde_json::Value;

/// Prints `value` as pretty JSON on stdout. Serialization of a plain
/// `serde_json::Value` cannot fail, so this is infallible in practice.
pub fn print_json(value: &Value) {
    match serde_json::to_string_pretty(value) {
        Ok(text) => println!("{text}"),
        Err(error) => eprintln!("failed to serialize output: {error}"),
    }
}

/// Formats a duration in milliseconds as `H:MM:SS` (hours dropped when zero),
/// for human-readable listings.
pub fn format_duration_ms(duration_ms: i64) -> String {
    let total_seconds = duration_ms.max(0) / 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}
