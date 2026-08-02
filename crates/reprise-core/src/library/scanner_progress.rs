//! Scan progress counting and reporting, split out of `scanner.rs` to keep it
//! under the project's 800-line rule — its test suite already lives in the
//! sibling `scanner_progress_tests.rs`. The previous catalog supplies a cheap
//! estimate without walking the source twice; `ScanProgressReporter` forwards
//! one monotone `Scanning` update per audio file to the caller's callback.

use std::path::Path;

use rusqlite::Connection;

use super::ScanProgress;

/// Estimates this scan's audio-file count from present catalog rows below
/// `root`. Reading the local database is cheap for every source, unlike an
/// exact pre-count that repeats every directory listing and doubles Android
/// SAF's Binder IPC. The component-aware Rust check is authoritative here;
/// this is progress metadata, so streaming the small catalog is preferable to
/// duplicating the scanner's more involved SQL path prefilter.
pub(crate) fn estimated_audio_files(
    conn: &Connection,
    root: &Path,
) -> Result<u64, rusqlite::Error> {
    let mut statement = conn.prepare(&format!(
        "SELECT path FROM tracks WHERE {}",
        crate::queries::PRESENT
    ))?;
    let paths = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut total = 0_u64;
    for path in paths {
        if Path::new(&path?).starts_with(root) {
            total = total.saturating_add(1);
        }
    }
    Ok(total)
}

/// Forwards per-file scan progress to the caller. `total` starts at the
/// previous catalog's estimate and is nudged up if the walk visits more files,
/// so `processed` never exceeds it. A first scan therefore grows its total as
/// files are discovered; a rescan begins with the last known catalog size.
pub(crate) struct ScanProgressReporter<'a> {
    callback: &'a mut dyn FnMut(ScanProgress),
    processed: u64,
    total: u64,
}

impl<'a> ScanProgressReporter<'a> {
    pub(crate) fn new(callback: &'a mut dyn FnMut(ScanProgress), total: u64) -> Self {
        Self {
            callback,
            processed: 0,
            total,
        }
    }

    pub(crate) fn advance(&mut self, path: &Path) {
        self.processed += 1;
        self.total = self.total.max(self.processed);
        (self.callback)(ScanProgress::Scanning {
            processed: self.processed,
            total: self.total,
            current_path: path.to_path_buf(),
        });
    }
}
