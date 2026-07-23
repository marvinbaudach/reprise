//! Scan progress counting and reporting, split out of `scanner.rs` to keep it
//! under the project's 800-line rule — its test suite already lives in the
//! sibling `scanner_progress_tests.rs`. `count_audio_files` sizes the walk up
//! front so a percentage is possible; `ScanProgressReporter` forwards one
//! monotone `Scanning` update per audio file to the caller's callback.

use std::path::Path;

use super::ScanProgress;

/// Counts the audio files under `root` up front so `scan_folder_with_progress`
/// can report a total. Uses the parent module's `is_audio_file` matcher so the
/// count and the walk agree on what "an audio file" is.
pub(crate) fn count_audio_files(root: &Path) -> u64 {
    walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file() && super::is_audio_file(entry.path()))
        .count() as u64
}

/// Forwards per-file scan progress to the caller. `total` starts at the
/// up-front `count_audio_files` estimate and is nudged up if the walk turns out
/// to visit more files than the pre-count saw, so `processed` never exceeds it.
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
