//! The worker half of remove-from-library and move-to-trash: runs off the GTK
//! thread against its own database handle.

use std::path::PathBuf;

use super::{DeleteMode, DeleteReport};

/// How many whole audio files trashing `tracks` moves to Trash, and how many
/// CUE tracks it only hides because their file holds tracks not selected.
pub(super) fn trash_counts(db: &reprise_core::db::Db, tracks: &[(i64, PathBuf)]) -> (usize, usize) {
    let plan = reprise_core::library::trash_tracks::plan_file_trash(db, tracks);
    (plan.files.len(), plan.hidden.len())
}

pub(super) fn run_delete(
    db: &reprise_core::db::Db,
    tracks: &[(i64, PathBuf)],
    mode: DeleteMode,
) -> DeleteReport {
    match mode {
        DeleteMode::Remove => {
            match reprise_core::queries::exclude_tracks_matching_paths(db, tracks, now_unix()) {
                Ok(removed_ids) => {
                    let failures = tracks.len().saturating_sub(removed_ids.len());
                    DeleteReport {
                        removed_ids,
                        hidden: 0,
                        failures,
                    }
                }
                Err(error) => {
                    tracing::error!(%error, "remove-from-library transaction failed");
                    DeleteReport {
                        removed_ids: Vec::new(),
                        hidden: 0,
                        failures: tracks.len(),
                    }
                }
            }
        }
        DeleteMode::Trash => {
            let report = match reprise_platform_linux::trash::Session::open() {
                Ok(session) => {
                    reprise_core::library::trash_tracks::trash_tracks_with(db, tracks, |path| {
                        session.delete(path)
                    })
                }
                Err(error) => {
                    reprise_core::library::trash_tracks::trash_tracks_with(db, tracks, |_| {
                        Err(error.clone())
                    })
                }
            };
            for failure in &report.failures {
                tracing::warn!(id = failure.id, path = %failure.path.display(), error = %failure.error, "move-to-trash failed");
            }
            DeleteReport {
                removed_ids: report.removed_ids,
                hidden: report.hidden_ids.len(),
                failures: report.failures.len(),
            }
        }
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}
