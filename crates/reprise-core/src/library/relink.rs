use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::db::Db;
use rusqlite::OptionalExtension;

use super::scanner::move_detect::MOVE_MATCH_TOLERANCE_MS;
use super::scanner::ScanError;
use super::source::{
    self, LibraryLinkMode, LibraryPathPresence, LibrarySource, LibraryWalkControl, LibraryWalkItem,
    LibraryWalkOrder, UnixLibrarySource,
};

#[path = "relink_source.rs"]
mod source_queries;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelinkMismatch {
    pub old_duration_ms: i64,
    pub new_duration_ms: i64,
    pub old_title: String,
    pub new_title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelinkTarget {
    pub track_id: i64,
    pub old_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FolderRelinkReport {
    pub relinked: u32,
    pub group_size: u32,
}

pub fn probe_relink(
    db: &Db,
    target: &RelinkTarget,
    new_path: &Path,
) -> Result<Option<RelinkMismatch>, ScanError> {
    probe_relink_with_source(&UnixLibrarySource, db, target, new_path)
}

pub fn probe_relink_with_source(
    source: &dyn LibrarySource,
    db: &Db,
    target: &RelinkTarget,
    new_path: &Path,
) -> Result<Option<RelinkMismatch>, ScanError> {
    let conn = db.conn();
    if source.probe(&target.old_path, LibraryLinkMode::Follow) != LibraryPathPresence::Absent {
        return Err(ScanError::RelinkTargetChanged {
            track_id: target.track_id,
        });
    }
    let (mut old_duration_ms, old_title, segment_index): (i64, String, i64) = conn
        .query_row(
            &format!(
                "SELECT duration_ms, title, segment_index FROM tracks \
                 WHERE id = ?1 AND path = ?2 AND {}",
                crate::queries::MISSING
            ),
            rusqlite::params![target.track_id, target.old_path.to_string_lossy()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or(ScanError::RelinkTargetChanged {
            track_id: target.track_id,
        })?;
    if segment_index > 0 {
        // A track cut from a file is compared by the length of the whole file,
        // which is where the last track ends; its title is the sheet's, not the
        // file's.
        old_duration_ms = conn.query_row(
            "SELECT coalesce(max(segment_end_ms), 0) FROM tracks WHERE path = ?1",
            [target.old_path.to_string_lossy()],
            |row| row.get(0),
        )?;
    }
    let meta = super::scanner::track_meta::read_meta(new_path)?;
    let new_title = (!meta.title.is_empty()).then_some(meta.title);
    let duration_mismatch =
        old_duration_ms.abs_diff(meta.duration_ms) > MOVE_MATCH_TOLERANCE_MS.unsigned_abs();
    let title_mismatch =
        segment_index == 0 && new_title.as_deref().is_some_and(|title| title != old_title);
    if !duration_mismatch && !title_mismatch {
        return Ok(None);
    }
    Ok(Some(RelinkMismatch {
        old_duration_ms,
        new_duration_ms: meta.duration_ms,
        old_title,
        new_title,
    }))
}

pub fn relink_track(db: &Db, target: &RelinkTarget, new_path: &Path) -> Result<(), ScanError> {
    relink_track_with_source(&UnixLibrarySource, db, target, new_path)
}

pub fn relink_track_with_source(
    source: &dyn LibrarySource,
    db: &Db,
    target: &RelinkTarget,
    new_path: &Path,
) -> Result<(), ScanError> {
    let conn = db.conn();
    if source.probe(&target.old_path, LibraryLinkMode::Follow) != LibraryPathPresence::Absent {
        return Err(ScanError::RelinkTargetChanged {
            track_id: target.track_id,
        });
    }
    let meta = super::scanner::track_meta::read_meta(new_path)?;
    let title = if meta.title.is_empty() {
        source.display_name(new_path).unwrap_or_default()
    } else {
        meta.title.clone()
    };
    let facts = source_queries::file_facts(source, new_path).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "relink source disappeared or lacks file facts: {}",
                new_path.display()
            ),
        )
    })?;
    let (device, inode) = facts.identity.map_or((None, None), |(device, inode)| {
        (Some(device as i64), Some(inode as i64))
    });
    let mount_point =
        super::mounts::mount_point_of(new_path).map(|path| path.to_string_lossy().into_owned());

    let tx = conn.unchecked_transaction()?;
    let segment_index: Option<i64> = tx
        .query_row(
            &format!(
                "SELECT segment_index FROM tracks WHERE id = ?1 AND path = ?2 AND {}",
                crate::queries::MISSING
            ),
            rusqlite::params![target.track_id, target.old_path.to_string_lossy()],
            |row| row.get(0),
        )
        .optional()?;
    let Some(segment_index) = segment_index else {
        return Err(ScanError::RelinkTargetChanged {
            track_id: target.track_id,
        });
    };
    let identity = super::scanner::move_detect::FileIdentity {
        file_mtime: facts.mtime,
        file_size: facts.size as i64,
        device,
        inode,
        mount_point,
    };
    if segment_index > 0 {
        // The file moves with all of its tracks; none of them takes the file's tags.
        super::scanner::move_detect::move_segment_rows(
            &tx,
            &target.old_path.to_string_lossy(),
            new_path,
            &identity,
        )?;
    } else {
        super::scanner::move_detect::apply_file_identity(
            &tx,
            target.track_id,
            new_path,
            &title,
            &meta,
            false,
            &identity,
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn relink_from_folder(
    db: &Db,
    folder: &Path,
    group: &[RelinkTarget],
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(u32, u32),
) -> Result<FolderRelinkReport, ScanError> {
    relink_from_folder_with_source(
        &UnixLibrarySource,
        db,
        folder,
        group,
        cancel,
        &mut on_progress,
    )
}

fn relink_from_folder_with_source(
    source: &dyn LibrarySource,
    db: &Db,
    folder: &Path,
    group: &[RelinkTarget],
    cancel: &AtomicBool,
    on_progress: &mut dyn FnMut(u32, u32),
) -> Result<FolderRelinkReport, ScanError> {
    let conn = db.conn();
    let mut expected_paths: HashMap<i64, PathBuf> = group
        .iter()
        .map(|target| (target.track_id, target.old_path.clone()))
        .collect();
    let mut remaining: HashSet<i64> = expected_paths.keys().copied().collect();
    let group_size = u32::try_from(remaining.len()).unwrap_or(u32::MAX);
    if remaining.is_empty() {
        return Ok(FolderRelinkReport {
            relinked: 0,
            group_size,
        });
    }

    let Some(total) = source_queries::count_folder_audio_files(source, folder, cancel)? else {
        return Ok(FolderRelinkReport {
            relinked: 0,
            group_size,
        });
    };
    let mut processed = 0_u32;
    let mut relinked = 0_u32;
    let mut walk_failure = None;
    source::walk_with(source, folder, LibraryWalkOrder::FileName, |item| {
        let result = (|| -> Result<LibraryWalkControl, ScanError> {
            let entry = match item {
                LibraryWalkItem::Entry(entry) => entry,
                LibraryWalkItem::Error(error) => {
                    return Err(std::io::Error::other(error.detail).into());
                }
            };
            if !entry.is_file || !super::scanner::is_audio_file(&entry.path) {
                return Ok(LibraryWalkControl::Continue);
            }
            if cancel.load(Ordering::Acquire) {
                return Ok(LibraryWalkControl::Stop);
            }
            processed = processed.saturating_add(1);
            let path = entry.path.as_path();
            let Some(facts) = entry
                .metadata
                .as_ref()
                .and_then(source_queries::FileFacts::from_metadata)
                .or_else(|| source_queries::file_facts(source, path))
            else {
                on_progress(processed, total);
                return Ok(LibraryWalkControl::Continue);
            };
            let identity = facts
                .identity
                .map(|(device, inode)| (device as i64, inode as i64));
            let meta = match super::scanner::track_meta::read_meta(path) {
                Ok(meta) => meta,
                Err(ScanError::Import { .. }) => {
                    on_progress(processed, total);
                    return Ok(LibraryWalkControl::Continue);
                }
                Err(error) => return Err(error),
            };
            let title = if meta.title.is_empty() {
                source.display_name(path).unwrap_or_default()
            } else {
                meta.title.clone()
            };
            let mount_point = super::mounts::mount_point_of(path)
                .map(|mount| mount.to_string_lossy().into_owned());
            let tx = conn.unchecked_transaction()?;
            let candidate = super::scanner::move_detect::find_move_candidate_in_with_source(
                source,
                &tx,
                &super::scanner::move_detect::MoveLookup {
                    identity,
                    title: &title,
                    artist: &meta.artist,
                    album: &meta.album,
                    duration_ms: meta.duration_ms,
                    file_size: facts.size as i64,
                    tracks_album: None,
                },
                &remaining,
            )?;
            if let Some(candidate) = candidate {
                let expected_path = expected_paths
                    .get(&candidate.id)
                    .expect("move candidates are restricted to remaining target ids");
                let still_missing = source.probe(expected_path, LibraryLinkMode::Follow)
                    == LibraryPathPresence::Absent
                    && tx
                        .query_row(
                            &format!(
                                "SELECT 1 FROM tracks WHERE id = ?1 AND path = ?2 AND {}",
                                crate::queries::MISSING
                            ),
                            rusqlite::params![candidate.id, expected_path.to_string_lossy()],
                            |_| Ok(()),
                        )
                        .optional()?
                        .is_some();
                if still_missing {
                    let (device, inode) = identity
                        .map_or((None, None), |(device, inode)| (Some(device), Some(inode)));
                    let file_identity = super::scanner::move_detect::FileIdentity {
                        file_mtime: facts.mtime,
                        file_size: facts.size as i64,
                        device,
                        inode,
                        mount_point,
                    };
                    if candidate.segmented {
                        super::scanner::move_detect::move_segment_rows(
                            &tx,
                            &candidate.path,
                            path,
                            &file_identity,
                        )?;
                    } else {
                        super::scanner::move_detect::apply_file_identity(
                            &tx,
                            candidate.id,
                            path,
                            &title,
                            &meta,
                            false,
                            &file_identity,
                        )?;
                    }
                }
                // A CUE file stands for all of its tracks at once, and each of
                // them counts as relinked, as `group_size` counts them.
                let settled: Vec<i64> = expected_paths
                    .iter()
                    .filter(|(id, old)| {
                        **id == candidate.id
                            || (candidate.segmented && old.to_string_lossy() == candidate.path)
                    })
                    .map(|(id, _)| *id)
                    .collect();
                if still_missing {
                    relinked = relinked
                        .saturating_add(u32::try_from(settled.len()).unwrap_or(u32::MAX));
                }
                for id in settled {
                    remaining.remove(&id);
                    expected_paths.remove(&id);
                }
            }
            tx.commit()?;
            on_progress(processed, total);
            if remaining.is_empty() {
                return Ok(LibraryWalkControl::Stop);
            }
            Ok(LibraryWalkControl::Continue)
        })();
        match result {
            Ok(control) => control,
            Err(error) => {
                walk_failure = Some(error);
                LibraryWalkControl::Stop
            }
        }
    });
    if let Some(error) = walk_failure {
        return Err(error);
    }
    Ok(FolderRelinkReport {
        relinked,
        group_size,
    })
}

#[cfg(test)]
#[path = "relink_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "relink_source_name_tests.rs"]
mod source_name_tests;
