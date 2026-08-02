//! Scan progress counting and reporting, split out of `scanner.rs` to keep it
//! under the project's 800-line rule — its test suite already lives in the
//! sibling `scanner_progress_tests.rs`. The previous catalog supplies a cheap
//! estimate without walking the source twice; `ScanProgressReporter` forwards
//! one monotone `Scanning` update per audio file to the caller's callback.

use std::path::Path;

use rusqlite::Connection;

use super::ScanProgress;

/// Estimates this scan's audio-file count from the present catalog rows below
/// `root`, or `None` when the catalog has nothing to estimate from — a first
/// scan of a fresh library, or a root never scanned before.
///
/// `None` is a real answer, not a zero: it means "no denominator exists yet",
/// and it travels all the way to the progress bar so the UI can say so instead
/// of showing a percentage it does not have. Reporting `Some(0)` here would
/// make [`ScanProgressReporter`] raise the total to match every file it visits,
/// and a first scan — the longest one a user ever waits through — would sit at
/// a full bar from its first file to its last.
///
/// Reading the local database is cheap for every source, unlike an exact
/// pre-count that repeats every directory listing and doubles Android SAF's
/// Binder IPC.
///
/// The `LIKE` prefilter narrows the rows SQLite hands back before Rust sees
/// them, exactly as `scanner_vanish::candidates_under_root` does and for the
/// same reason — the catalog is this app's largest table, and a rescan of one
/// small subtree must not stream every present track in the library through
/// Rust to size a progress bar. As there, `LIKE` is only a *superset* filter:
/// `Path::starts_with` remains the authoritative check, so the result is
/// identical to a full-table scan regardless of `LIKE`'s case-insensitivity,
/// and the `/` before `%` keeps a sibling root (`/music2`) out of `/music`.
pub(crate) fn estimated_audio_files(
    conn: &Connection,
    root: &Path,
) -> Result<Option<u64>, rusqlite::Error> {
    let root_str = root.to_string_lossy();
    let pattern = format!(
        "{}/%",
        crate::library::playlists::escape_like(root_str.trim_end_matches('/'))
    );
    let mut statement = conn.prepare(&format!(
        "SELECT path FROM tracks WHERE {} AND path LIKE ?1 ESCAPE '\\'",
        crate::queries::PRESENT
    ))?;
    let paths = statement.query_map(rusqlite::params![pattern], |row| row.get::<_, String>(0))?;
    let mut total = 0_u64;
    for path in paths {
        if Path::new(&path?).starts_with(root) {
            total = total.saturating_add(1);
        }
    }
    Ok((total > 0).then_some(total))
}

/// Forwards per-file scan progress to the caller. `total` starts at the
/// previous catalog's estimate and is nudged up if the walk visits more files,
/// so `processed` never exceeds it. A rescan therefore begins with the last
/// known catalog size; a first scan has no estimate at all and keeps reporting
/// `None`, which the UI renders as indeterminate rather than as a percentage.
pub(crate) struct ScanProgressReporter<'a> {
    callback: &'a mut dyn FnMut(ScanProgress),
    processed: u64,
    total: Option<u64>,
}

impl<'a> ScanProgressReporter<'a> {
    pub(crate) fn new(callback: &'a mut dyn FnMut(ScanProgress), total: Option<u64>) -> Self {
        Self {
            callback,
            processed: 0,
            total,
        }
    }

    pub(crate) fn advance(&mut self, path: &Path) {
        self.processed += 1;
        // An estimate that turns out too small grows to keep `processed`
        // inside it. No estimate stays no estimate — inventing one from the
        // files seen so far would report completion for every file.
        self.total = self.total.map(|total| total.max(self.processed));
        (self.callback)(ScanProgress::Scanning {
            processed: self.processed,
            total: self.total,
            current_path: path.to_path_buf(),
        });
    }
}
