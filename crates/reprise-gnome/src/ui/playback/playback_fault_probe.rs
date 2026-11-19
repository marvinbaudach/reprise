//! The filesystem probe behind a playback fault (`playback_faults.rs`).
//!
//! Deciding whether a faulted track's file is gone, and marking it missing,
//! touches the file's mount: `is_file()`, `std::fs::metadata` and the
//! `symlink_metadata` walk behind `mark_track_missing_if_current`. On a dead
//! hard NFS or SMB mount every one of them blocks in the kernel, so none may run
//! on the main thread. The handler runs the probe through `gio::spawn_blocking`,
//! which is why the probe is an `Arc<dyn Fn + Send + Sync>` over plain owned
//! values: no `Rc`, widget or `RefCell` borrow can cross into the job.
//!
//! The controller holds the probe in a field and defaults it to [`disk_probe`];
//! a test replaces it with one that blocks, answers a fixed verdict, or counts
//! its calls.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use reprise_core::db::Db;
use reprise_core::queries;

/// Everything the probe needs, by value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::ui) struct FaultProbeRequest {
    pub(in crate::ui) track_id: i64,
    pub(in crate::ui) path: PathBuf,
    /// The library file, so the job can open a handle of its own: a `Db` is not
    /// `Sync`. `None` for an in-memory library, which cannot be shared.
    pub(in crate::ui) db_path: Option<PathBuf>,
}

/// What the probe found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum FaultProbeAnswer {
    /// The path still names a file: the backend, not the disk, failed.
    FilePresent,
    /// The path names no file. `marked` is whether this probe changed the
    /// track's row to missing; `false` also covers a reconciled row and a
    /// failed write, both of which are logged by the probe.
    FileMissing { marked: bool },
}

pub(in crate::ui) type FaultProbe =
    Arc<dyn Fn(FaultProbeRequest) -> FaultProbeAnswer + Send + Sync>;

/// The production probe: `is_file()`, then — only when the file is gone —
/// `mark_track_missing_if_current` on a connection the job opens itself.
pub(in crate::ui) fn disk_probe() -> FaultProbe {
    Arc::new(|request| probe_disk(&request))
}

fn probe_disk(request: &FaultProbeRequest) -> FaultProbeAnswer {
    if request.path.is_file() {
        return FaultProbeAnswer::FilePresent;
    }
    FaultProbeAnswer::FileMissing {
        marked: mark_missing(request),
    }
}

fn mark_missing(request: &FaultProbeRequest) -> bool {
    let Some(db_path) = request.db_path.as_deref() else {
        tracing::warn!(
            track_id = request.track_id,
            "the library has no file to share with the probe; not marking the track missing"
        );
        return false;
    };
    let db = match Db::open_ready(db_path) {
        Ok(db) => db,
        Err(error) => {
            tracing::error!(%error, track_id = request.track_id, "failed to open the library to mark a track missing");
            return false;
        }
    };
    marked_by(&db, request.track_id, &request.path)
}

fn marked_by(db: &Db, track_id: i64, path: &Path) -> bool {
    match queries::mark_track_missing_if_current(db, track_id, path) {
        Ok(true) => true,
        Ok(false) => {
            tracing::info!(
                track_id,
                "stale playback fault did not mark a reconciled track missing"
            );
            false
        }
        Err(error) => {
            tracing::error!(%error, track_id, "failed to mark track missing");
            false
        }
    }
}
