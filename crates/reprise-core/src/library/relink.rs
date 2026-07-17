use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::Connection;
use rusqlite::OptionalExtension;

use super::scanner::ScanError;

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
    conn: &Connection,
    target: &RelinkTarget,
    new_path: &Path,
) -> Result<Option<RelinkMismatch>, ScanError> {
    let (old_duration_ms, old_title): (i64, String) = conn
        .query_row(
            &format!(
                "SELECT duration_ms, title FROM tracks \
                 WHERE id = ?1 AND path = ?2 AND {}",
                crate::queries::MISSING
            ),
            rusqlite::params![target.track_id, target.old_path.to_string_lossy()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or(ScanError::RelinkTargetChanged {
            track_id: target.track_id,
        })?;
    let meta = super::scanner::track_meta::read_meta(new_path)?;
    let new_title = (!meta.title.is_empty()).then_some(meta.title);
    let duration_mismatch = old_duration_ms.abs_diff(meta.duration_ms) > 2_000;
    let title_mismatch = new_title.as_deref().is_some_and(|title| title != old_title);
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

pub fn relink_track(
    conn: &mut Connection,
    target: &RelinkTarget,
    new_path: &Path,
) -> Result<(), ScanError> {
    let meta = super::scanner::track_meta::read_meta(new_path)?;
    let title = if meta.title.is_empty() {
        new_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .to_string()
    } else {
        meta.title.clone()
    };
    let (file_size, device, inode) = super::scanner::file_stat(new_path).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("relink source disappeared: {}", new_path.display()),
        )
    })?;
    let mount_point =
        super::mounts::mount_point_of(new_path).map(|path| path.to_string_lossy().into_owned());

    let tx = conn.transaction()?;
    let still_missing = tx
        .query_row(
            &format!(
                "SELECT 1 FROM tracks WHERE id = ?1 AND path = ?2 AND {}",
                crate::queries::MISSING
            ),
            rusqlite::params![target.track_id, target.old_path.to_string_lossy()],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !still_missing {
        return Err(ScanError::RelinkTargetChanged {
            track_id: target.track_id,
        });
    }
    super::scanner::move_detect::apply_file_identity(
        &tx,
        target.track_id,
        new_path,
        &title,
        &meta,
        false,
        &super::scanner::move_detect::FileIdentity {
            file_mtime: super::scanner::file_mtime(new_path),
            file_size: file_size as i64,
            device: Some(device as i64),
            inode: Some(inode as i64),
            mount_point,
        },
    )?;
    tx.commit()?;
    Ok(())
}

pub fn relink_from_folder(
    conn: &mut Connection,
    folder: &Path,
    group: &[RelinkTarget],
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(u32, u32),
) -> Result<FolderRelinkReport, ScanError> {
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

    let total = u32::try_from(super::scanner::count_audio_files(folder)).unwrap_or(u32::MAX);
    let mut processed = 0_u32;
    let mut relinked = 0_u32;
    for entry in walkdir::WalkDir::new(folder)
        .follow_links(false)
        .sort_by_file_name()
    {
        let entry = entry.map_err(|error| std::io::Error::other(error.to_string()))?;
        if !entry.file_type().is_file() || !super::scanner::is_audio_file(entry.path()) {
            continue;
        }
        if cancel.load(Ordering::Acquire) {
            break;
        }
        processed = processed.saturating_add(1);
        let path = entry.path();
        let Some((file_size, device, inode)) = super::scanner::file_stat(path) else {
            on_progress(processed, total);
            continue;
        };
        let meta = match super::scanner::track_meta::read_meta(path) {
            Ok(meta) => meta,
            Err(ScanError::Import { .. }) => {
                on_progress(processed, total);
                continue;
            }
            Err(error) => return Err(error),
        };
        let title = if meta.title.is_empty() {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default()
                .to_string()
        } else {
            meta.title.clone()
        };
        let mount_point =
            super::mounts::mount_point_of(path).map(|mount| mount.to_string_lossy().into_owned());
        let tx = conn.transaction()?;
        let candidate = super::scanner::move_detect::find_move_candidate_in(
            &tx,
            &super::scanner::move_detect::MoveLookup {
                device: device as i64,
                inode: inode as i64,
                title: &title,
                artist: &meta.artist,
                album: &meta.album,
                duration_ms: meta.duration_ms,
                file_size: file_size as i64,
            },
            &remaining,
        )?;
        if let Some(candidate) = candidate {
            let expected_path = expected_paths
                .get(&candidate.id)
                .expect("move candidates are restricted to remaining target ids");
            let still_missing = tx
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
                super::scanner::move_detect::apply_file_identity(
                    &tx,
                    candidate.id,
                    path,
                    &title,
                    &meta,
                    false,
                    &super::scanner::move_detect::FileIdentity {
                        file_mtime: super::scanner::file_mtime(path),
                        file_size: file_size as i64,
                        device: Some(device as i64),
                        inode: Some(inode as i64),
                        mount_point,
                    },
                )?;
                relinked = relinked.saturating_add(1);
            }
            remaining.remove(&candidate.id);
            expected_paths.remove(&candidate.id);
        }
        tx.commit()?;
        on_progress(processed, total);
        if remaining.is_empty() {
            break;
        }
    }
    Ok(FolderRelinkReport {
        relinked,
        group_size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lofty::prelude::*;
    use lofty::tag::{Tag, TagType};
    use std::sync::atomic::AtomicBool;

    fn fixture_copy(dir: &Path, name: &str) -> std::path::PathBuf {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
        let destination = dir.join(name);
        std::fs::copy(source, &destination).unwrap();
        destination
    }

    fn tag_file(path: &Path, title: &str) {
        let mut tag = Tag::new(TagType::VorbisComments);
        tag.set_title(title.into());
        tag.save_to_path(path, lofty::config::WriteOptions::default())
            .unwrap();
    }

    fn imported_missing_track(
        title: &str,
    ) -> (tempfile::TempDir, Connection, i64, std::path::PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let old_path = fixture_copy(temp.path(), "old.flac");
        tag_file(&old_path, title);
        let mut conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        super::super::scanner::scan_folder(&mut conn, temp.path()).unwrap();
        let track_id = conn
            .query_row(
                "SELECT id FROM tracks WHERE path = ?1",
                [old_path.to_string_lossy().as_ref()],
                |row| row.get(0),
            )
            .unwrap();
        let new_path = temp.path().join("new.flac");
        std::fs::rename(old_path, &new_path).unwrap();
        conn.execute(
            "UPDATE tracks SET missing_since = 10, missing_reason = 'deleted' WHERE id = ?1",
            [track_id],
        )
        .unwrap();
        (temp, conn, track_id, new_path)
    }

    fn target_for(conn: &Connection, track_id: i64) -> RelinkTarget {
        let old_path = conn
            .query_row("SELECT path FROM tracks WHERE id = ?1", [track_id], |row| {
                row.get::<_, String>(0)
            })
            .unwrap();
        RelinkTarget {
            track_id,
            old_path: old_path.into(),
        }
    }

    fn targets_for(conn: &Connection, track_ids: &[i64]) -> Vec<RelinkTarget> {
        track_ids
            .iter()
            .map(|track_id| target_for(conn, *track_id))
            .collect()
    }

    fn moved_group(count: usize) -> (tempfile::TempDir, Connection, Vec<i64>, std::path::PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let old_folder = temp.path().join("old");
        std::fs::create_dir(&old_folder).unwrap();
        for index in 0..count {
            let path = fixture_copy(&old_folder, &format!("{index:02}.flac"));
            tag_file(&path, &format!("Track {index}"));
        }
        let mut conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        super::super::scanner::scan_folder(&mut conn, &old_folder).unwrap();
        let ids = {
            let mut statement = conn.prepare("SELECT id FROM tracks ORDER BY path").unwrap();
            statement
                .query_map([], |row| row.get(0))
                .unwrap()
                .collect::<Result<Vec<i64>, _>>()
                .unwrap()
        };
        let new_folder = temp.path().join("new");
        std::fs::rename(&old_folder, &new_folder).unwrap();
        conn.execute(
            "UPDATE tracks SET missing_since = 10, missing_reason = 'deleted'",
            [],
        )
        .unwrap();
        (temp, conn, ids, new_folder)
    }

    #[test]
    fn probe_accepts_duration_delta_at_matcher_tolerance() {
        let (_temp, conn, track_id, new_path) = imported_missing_track("Same recording");
        let target = target_for(&conn, track_id);
        conn.execute(
            "UPDATE tracks SET duration_ms = duration_ms + 2000 WHERE id = ?1",
            [track_id],
        )
        .unwrap();

        assert_eq!(probe_relink(&conn, &target, &new_path).unwrap(), None);
    }

    #[test]
    fn probe_reports_old_and_new_values_for_a_mismatch() {
        let (_temp, conn, track_id, new_path) = imported_missing_track("Old title");
        let target = target_for(&conn, track_id);
        tag_file(&new_path, "New title");
        conn.execute(
            "UPDATE tracks SET duration_ms = duration_ms + 3000 WHERE id = ?1",
            [track_id],
        )
        .unwrap();
        let old_duration_ms = conn
            .query_row(
                "SELECT duration_ms FROM tracks WHERE id = ?1",
                [track_id],
                |row| row.get(0),
            )
            .unwrap();
        let new_duration_ms = super::super::scanner::track_meta::read_meta(&new_path)
            .unwrap()
            .duration_ms;

        assert_eq!(
            probe_relink(&conn, &target, &new_path).unwrap(),
            Some(RelinkMismatch {
                old_duration_ms,
                new_duration_ms,
                old_title: "Old title".into(),
                new_title: Some("New title".into()),
            })
        );
    }

    #[test]
    fn probe_warns_when_only_the_readable_title_changed() {
        let (_temp, conn, track_id, new_path) = imported_missing_track("Old title");
        let target = target_for(&conn, track_id);
        tag_file(&new_path, "Different title");

        let mismatch = probe_relink(&conn, &target, &new_path)
            .unwrap()
            .expect("a readable changed title must warn even at matching duration");
        assert_eq!(mismatch.old_title, "Old title");
        assert_eq!(mismatch.new_title.as_deref(), Some("Different title"));
    }

    #[test]
    fn relink_preserves_user_data_and_refreshes_the_existing_row() {
        let (_temp, mut conn, track_id, new_path) = imported_missing_track("Relinked title");
        let target = target_for(&conn, track_id);
        conn.execute(
            "UPDATE tracks SET rating = 4, play_count = 9 WHERE id = ?1",
            [track_id],
        )
        .unwrap();

        relink_track(&mut conn, &target, &new_path).unwrap();

        let row: (
            i64,
            String,
            i64,
            i64,
            Option<i64>,
            Option<i64>,
            Option<String>,
        ) = conn
            .query_row(
                "SELECT id, path, rating, play_count, missing_since, removed_at, mount_point \
                 FROM tracks WHERE id = ?1",
                [track_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(row.0, track_id);
        assert_eq!(row.1, new_path.to_string_lossy());
        assert_eq!((row.2, row.3), (4, 9));
        assert_eq!((row.4, row.5), (None, None));
        assert!(row.6.is_some(), "relink must record the reachable mount");
    }

    #[test]
    fn stale_relink_does_not_overwrite_a_track_resurrected_while_dialog_was_open() {
        let (_temp, mut conn, track_id, new_path) = imported_missing_track("Original title");
        let target = target_for(&conn, track_id);
        let original_path: String = conn
            .query_row("SELECT path FROM tracks WHERE id = ?1", [track_id], |row| {
                row.get(0)
            })
            .unwrap();
        conn.execute(
            "UPDATE tracks SET missing_since = NULL, missing_reason = NULL WHERE id = ?1",
            [track_id],
        )
        .unwrap();

        assert!(matches!(
            relink_track(&mut conn, &target, &new_path),
            Err(ScanError::RelinkTargetChanged { track_id: changed }) if changed == track_id
        ));
        let path_after: String = conn
            .query_row("SELECT path FROM tracks WHERE id = ?1", [track_id], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(path_after, original_path);
    }

    #[test]
    fn stale_relink_does_not_overwrite_a_reused_missing_track_id() {
        let (_temp, mut conn, track_id, new_path) = imported_missing_track("Original title");
        let old_path: String = conn
            .query_row("SELECT path FROM tracks WHERE id = ?1", [track_id], |row| {
                row.get(0)
            })
            .unwrap();
        conn.execute("DELETE FROM tracks WHERE id = ?1", [track_id])
            .unwrap();
        conn.execute(
            "INSERT INTO tracks (id, path, title, added_at, missing_since, missing_reason) \
             VALUES (?1, '/different/reused.flac', 'Different track', 1, 20, 'deleted')",
            [track_id],
        )
        .unwrap();
        let target = RelinkTarget {
            track_id,
            old_path: old_path.into(),
        };

        assert!(matches!(
            relink_track(&mut conn, &target, &new_path),
            Err(ScanError::RelinkTargetChanged { track_id: changed }) if changed == track_id
        ));
    }

    #[test]
    fn folder_relink_matches_every_track_in_the_selected_missing_group() {
        let (_temp, mut conn, ids, new_folder) = moved_group(3);
        let targets = targets_for(&conn, &ids);
        let cancel = AtomicBool::new(false);
        let mut progress = Vec::new();

        let report = relink_from_folder(
            &mut conn,
            &new_folder,
            &targets,
            &cancel,
            |processed, total| progress.push((processed, total)),
        )
        .unwrap();

        assert_eq!(
            report,
            FolderRelinkReport {
                relinked: 3,
                group_size: 3,
            }
        );
        assert_eq!(progress.last(), Some(&(3, 3)));
        let present: i64 = conn
            .query_row(
                "SELECT count(*) FROM tracks WHERE missing_since IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(present, 3);
    }

    #[test]
    fn folder_relink_stops_immediately_after_the_last_group_match() {
        let (_temp, mut conn, ids, new_folder) = moved_group(2);
        let targets = targets_for(&conn, &ids);
        let foreign = fixture_copy(&new_folder, "zz-foreign.flac");
        tag_file(&foreign, "Not in the library");
        let cancel = AtomicBool::new(false);
        let mut progress = Vec::new();

        let report = relink_from_folder(
            &mut conn,
            &new_folder,
            &targets,
            &cancel,
            |processed, total| progress.push((processed, total)),
        )
        .unwrap();

        assert_eq!(report.relinked, 2);
        assert_eq!(progress, vec![(1, 3), (2, 3)]);
    }

    #[test]
    fn folder_relink_never_imports_an_unmatched_audio_file() {
        let (_temp, mut conn, ids, new_folder) = moved_group(1);
        let targets = targets_for(&conn, &ids);
        let foreign = fixture_copy(&new_folder, "00-foreign.flac");
        tag_file(&foreign, "Foreign file");
        let cancel = AtomicBool::new(false);

        let report =
            relink_from_folder(&mut conn, &new_folder, &targets, &cancel, |_, _| {}).unwrap();

        assert_eq!(report.relinked, 1);
        let rows: i64 = conn
            .query_row("SELECT count(*) FROM tracks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, 1, "folder relink must never become an import");
    }

    #[test]
    fn folder_relink_checks_cancellation_before_each_audio_file() {
        let (_temp, mut conn, ids, new_folder) = moved_group(3);
        let targets = targets_for(&conn, &ids);
        let cancel = AtomicBool::new(false);
        let mut progress = Vec::new();

        let report = relink_from_folder(
            &mut conn,
            &new_folder,
            &targets,
            &cancel,
            |processed, total| {
                progress.push((processed, total));
                cancel.store(true, Ordering::Release);
            },
        )
        .unwrap();

        assert_eq!(report.relinked, 1);
        assert_eq!(report.group_size, 3);
        assert_eq!(progress, vec![(1, 3)]);
    }

    #[test]
    fn folder_relink_cannot_attach_a_file_to_a_missing_track_outside_the_group() {
        let (_temp, mut conn, ids, new_folder) = moved_group(2);
        let selected = [target_for(&conn, ids[0])];
        let cancel = AtomicBool::new(false);

        let report =
            relink_from_folder(&mut conn, &new_folder, &selected, &cancel, |_, _| {}).unwrap();

        assert_eq!(report.relinked, 1);
        let selected_missing: Option<i64> = conn
            .query_row(
                "SELECT missing_since FROM tracks WHERE id = ?1",
                [ids[0]],
                |row| row.get(0),
            )
            .unwrap();
        let unselected_missing: Option<i64> = conn
            .query_row(
                "SELECT missing_since FROM tracks WHERE id = ?1",
                [ids[1]],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(selected_missing, None);
        assert_eq!(unselected_missing, Some(10));
    }

    #[test]
    fn folder_relink_rechecks_path_identity_before_writing_a_match() {
        let (_temp, mut conn, ids, new_folder) = moved_group(1);
        let target = target_for(&conn, ids[0]);
        let identity: (i64, i64, i64, i64) = conn
            .query_row(
                "SELECT duration_ms, file_size, device, inode FROM tracks WHERE id = ?1",
                [ids[0]],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        conn.execute("DELETE FROM tracks WHERE id = ?1", [ids[0]])
            .unwrap();
        conn.execute(
            "INSERT INTO tracks (id, path, title, duration_ms, file_size, device, inode, \
             added_at, missing_since, missing_reason) \
             VALUES (?1, '/different/reused.flac', 'Track 0', ?2, ?3, ?4, ?5, 1, 20, 'deleted')",
            rusqlite::params![ids[0], identity.0, identity.1, identity.2, identity.3],
        )
        .unwrap();

        let report = relink_from_folder(
            &mut conn,
            &new_folder,
            &[target],
            &AtomicBool::new(false),
            |_, _| {},
        )
        .unwrap();

        assert_eq!(report.relinked, 0);
        let path: String = conn
            .query_row("SELECT path FROM tracks WHERE id = ?1", [ids[0]], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(path, "/different/reused.flac");
    }
}
