//! Temporary files a sync run hands to a device backend.
//!
//! A backend copies *from a path*, so anything a run produces in memory — an
//! encoded analysis sidecar, the track metadata list — has to reach the disk
//! before it can reach the device. The same is true in reverse for a
//! transcode, which writes its output somewhere before that output is copied.
//!
//! Those files are staged under a name no other run and no other process can
//! pick, and removed again once the backend is done reading them. Writing and
//! removing live here rather than in a frontend: the frontend decides *when*
//! to stage something, never *where* or *how*.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Separates the staged files of one run from the next within this process;
/// the process id in the name separates them from every other process.
static SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// A path in the system temporary directory that no concurrent staging call
/// can return twice.
///
/// The device id is sanitized before it becomes part of a filename, because a
/// GVfs device identifier is free-form and may carry separators.
pub fn temporary_path(device_id: &str, track_id: i64, extension: &str) -> PathBuf {
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let safe_device = super::safe_component(device_id, "device");
    std::env::temp_dir().join(format!(
        "reprise-sync-{safe_device}-{}-{track_id}-{sequence}.{extension}",
        std::process::id(),
    ))
}

/// Writes `bytes` to a fresh [`temporary_path`] and answers with that path.
///
/// A failure answers with the error and no path, so a half-written file can
/// never be handed to a backend as if it were the encoded data: the caller has
/// nothing to hand over. What the failed write left behind stays where it is —
/// the name is unique to this call, so nothing will ever read it again.
pub fn stage_bytes(
    device_id: &str,
    track_id: i64,
    extension: &str,
    bytes: &[u8],
) -> std::io::Result<PathBuf> {
    let path = temporary_path(device_id, track_id, extension);
    std::fs::write(&path, bytes)?;
    Ok(path)
}

/// Removes a staged file once the backend has read it.
///
/// Silent by design: the copy it belonged to has already reported its own
/// outcome, and a leftover file in the temporary directory is not something to
/// tell the user about on top of that.
pub fn discard(path: &Path) {
    let _ = std::fs::remove_file(path);
}

#[cfg(test)]
#[path = "staging_tests.rs"]
mod tests;
