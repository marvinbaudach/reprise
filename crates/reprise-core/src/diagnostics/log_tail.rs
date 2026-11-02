//! A bounded look at the end of a log file, for the diagnostics report.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crate::crash_report::CRASH_REPORT_MARKER;

/// The most lines of one log the report carries.
pub const TAIL_MAX_LINES: usize = 60;
/// The most bytes read from the end of one log file.
pub const TAIL_MAX_BYTES: u64 = 16 * 1024;

/// The most bytes read from the end of a log while looking for a crash report.
/// A crash report ends the run, so it sits near the end, but its backtrace
/// alone can outgrow [`TAIL_MAX_BYTES`].
pub const CRASH_SCAN_MAX_BYTES: u64 = 256 * 1024;
/// The longest line a crash section keeps, so a huge panic message cannot
/// push the lines after it out of the byte bound.
const CRASH_LINE_MAX_BYTES: usize = 1024;

/// Reads the end of the log at `path`, bounded by [`TAIL_MAX_BYTES`] and
/// [`TAIL_MAX_LINES`]. A log whose run ended in a crash report starts at that
/// report instead, so its message and location survive a long backtrace. A
/// missing or unreadable file, or an empty one, gives `None`: a report
/// without a tail is better than no report.
#[must_use]
pub fn read_log_tail(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    let start = length.saturating_sub(CRASH_SCAN_MAX_BYTES);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.take(CRASH_SCAN_MAX_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    if let Some(section) = crash_section_of(&String::from_utf8_lossy(&bytes)) {
        return Some(section);
    }
    let window = bytes.len().saturating_sub(TAIL_MAX_BYTES as usize);
    let tail = bytes.get(window..)?;
    tail_of(&String::from_utf8_lossy(tail), start > 0 || window > 0)
}

/// From the last crash report marker in `text` on, bounded by
/// [`TAIL_MAX_LINES`] and [`TAIL_MAX_BYTES`]. The bound cuts the end of the
/// section, the backtrace, never the header the report opens with. `None`
/// when `text` holds no crash report.
#[must_use]
pub fn crash_section_of(text: &str) -> Option<String> {
    let marker_at = text
        .match_indices(CRASH_REPORT_MARKER)
        .map(|(index, _)| index)
        .filter(|&index| index == 0 || text.as_bytes().get(index - 1) == Some(&b'\n'))
        .last()?;
    let mut kept = Vec::new();
    let mut bytes = 0;
    for line in text.get(marker_at..)?.lines().take(TAIL_MAX_LINES) {
        let line = truncated(line, CRASH_LINE_MAX_BYTES);
        bytes += line.len() + 1;
        if bytes > TAIL_MAX_BYTES as usize {
            break;
        }
        kept.push(line);
    }
    Some(kept.join("\n"))
}

/// `line` cut to at most `max` bytes, on a character boundary.
fn truncated(line: &str, max: usize) -> &str {
    let mut end = line.len().min(max);
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    line.get(..end).unwrap_or_default()
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
