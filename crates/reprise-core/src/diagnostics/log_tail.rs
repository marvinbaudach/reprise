//! A bounded look at the end of a log file, for the diagnostics report.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// The most lines of one log the report carries.
pub const TAIL_MAX_LINES: usize = 60;
/// The most bytes read from the end of one log file.
pub const TAIL_MAX_BYTES: u64 = 16 * 1024;

/// Reads the end of the log at `path`, bounded by [`TAIL_MAX_BYTES`] and
/// [`TAIL_MAX_LINES`]. A missing or unreadable file, or an empty one, gives
/// `None`: a report without a tail is better than no report.
#[must_use]
pub fn read_log_tail(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    let start = length.saturating_sub(TAIL_MAX_BYTES);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.take(TAIL_MAX_BYTES).read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    tail_of(&text, start > 0)
}

/// The last [`TAIL_MAX_LINES`] lines of `text`. With `cut_at_start` the first
/// line may be a fragment of a longer one and is dropped. Blank text gives
/// `None`.
#[must_use]
pub fn tail_of(text: &str, cut_at_start: bool) -> Option<String> {
    let mut lines: Vec<&str> = text.lines().collect();
    if cut_at_start && !lines.is_empty() {
        lines.remove(0);
    }
    let first = lines.len().saturating_sub(TAIL_MAX_LINES);
    let kept = lines.get(first..)?;
    let tail = kept.join("\n");
    (!tail.trim().is_empty()).then_some(tail)
}
