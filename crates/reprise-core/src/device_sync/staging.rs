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

use std::fmt;
use std::io;
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
pub fn staging_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("reprise")
        .join("device-sync")
}

pub fn temporary_path(device_id: &str, track_id: i64, extension: &str) -> PathBuf {
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let safe_device = super::safe_component(device_id, "device");
    let directory = staging_dir();
    let _ = std::fs::create_dir_all(&directory);
    directory.join(format!(
        "reprise-sync-{}-{safe_device}-{track_id}-{sequence}.{extension}",
        std::process::id()
    ))
}

#[derive(Debug)]
pub struct StagingError {
    directory: PathBuf,
    source: io::Error,
}

impl StagingError {
    fn new(directory: &Path, source: io::Error) -> Self {
        Self {
            directory: directory.to_path_buf(),
            source,
        }
    }
}

impl fmt::Display for StagingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.source.kind() == io::ErrorKind::StorageFull {
            return write!(
                formatter,
                "local staging directory '{}' is full",
                self.directory.display()
            );
        }
        write!(
            formatter,
            "could not write local staging data in '{}': {}",
            self.directory.display(),
            self.source
        )
    }
}

impl std::error::Error for StagingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
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
) -> Result<PathBuf, StagingError> {
    let directory = staging_dir();
    stage_bytes_with(
        &directory,
        device_id,
        track_id,
        extension,
        bytes,
        |path, contents| std::fs::write(path, contents),
    )
}

fn stage_bytes_with(
    directory: &Path,
    device_id: &str,
    track_id: i64,
    extension: &str,
    bytes: &[u8],
    write: impl FnOnce(&Path, &[u8]) -> io::Result<()>,
) -> Result<PathBuf, StagingError> {
    std::fs::create_dir_all(directory).map_err(|error| StagingError::new(directory, error))?;
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let safe_device = super::safe_component(device_id, "device");
    let path = directory.join(format!(
        "reprise-sync-{}-{safe_device}-{track_id}-{sequence}.{extension}",
        std::process::id()
    ));
    if let Err(error) = write(&path, bytes) {
        let _ = std::fs::remove_file(&path);
        return Err(StagingError::new(directory, error));
    }
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

/// Counts the sync runs in flight anywhere in this process.
static ACTIVE_RUNS: AtomicU64 = AtomicU64::new(0);

/// Marks one sync run as in flight for as long as it is held.
///
/// `cleanup_process_files` sweeps by process, but a process can hold more than
/// one runtime — the GNOME test binary drives several at once, and they share
/// both the process id and the device id that make up a staged name. Counting
/// the runs keeps the sweep's reach and the "nothing is running" question on
/// the same scope. Without it a finishing run deletes the file a concurrent
/// one is still copying, and that run fails with a missing staged path.
pub struct ActiveRun(());

impl ActiveRun {
    pub fn begin() -> Self {
        ACTIVE_RUNS.fetch_add(1, Ordering::SeqCst);
        Self(())
    }
}

impl Drop for ActiveRun {
    fn drop(&mut self) {
        ACTIVE_RUNS.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Removes staged files owned by this process, once no run still needs them.
///
/// Every staged file is discarded by the run that made it; this sweep only
/// collects what an interrupted run left behind. It is therefore always safe
/// to skip, and never safe to run while a sibling is mid-copy.
pub fn cleanup_process_files() {
    if ACTIVE_RUNS.load(Ordering::SeqCst) > 0 {
        return;
    }
    let directory = staging_dir();
    let prefix = format!("reprise-sync-{}-", std::process::id());
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let owned = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(&prefix));
        if owned && path.is_file() {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
#[path = "staging_tests.rs"]
mod tests;
