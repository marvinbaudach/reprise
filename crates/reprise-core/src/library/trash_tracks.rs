//! Platform-independent move-to-trash reconciliation. The caller injects
//! its platform trash action; tests inject scratch-only actions.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rusqlite::OptionalExtension;

use crate::db::Db;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashFailure {
    pub id: i64,
    pub path: PathBuf,
    pub error: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TrashReport {
    pub removed_ids: Vec<i64>,
    pub failures: Vec<TrashFailure>,
}

/// Requests that still match a library row, and the ones that already do not.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TrashPlan {
    pub validated: Vec<(i64, PathBuf)>,
    pub failures: Vec<TrashFailure>,
}

/// De-duplicates ids and refuses paths that no longer match the library row.
pub fn plan_trash(db: &Db, tracks: &[(i64, PathBuf)]) -> TrashPlan {
    let conn = db.conn();
    let mut plan = TrashPlan::default();
    let mut seen = HashSet::new();

    for (id, path) in tracks {
        if !seen.insert(*id) {
            continue;
        }
        let registered = conn
            .query_row("SELECT path FROM tracks WHERE id=?1", [id], |row| {
                row.get::<_, String>(0)
            })
            .optional();
        match registered {
            Ok(Some(registered)) if registered == path.to_string_lossy() => {
                plan.validated.push((*id, path.clone()));
            }
            Ok(_) => {
                plan.failures.push(TrashFailure {
                    id: *id,
                    path: path.clone(),
                    error: "track path changed before trash; refusing stale request".into(),
                });
            }
            Err(error) => {
                plan.failures.push(TrashFailure {
                    id: *id,
                    path: path.clone(),
                    error: format!("could not validate track path before trash: {error}"),
                });
            }
        }
    }

    plan
}

/// Removes rows for files the caller actually moved to trash.
///
/// The caller must put only files confirmed as moved to trash in `trashed`.
/// `failures` is preserved in order, and cleanup failures discovered here are
/// appended after it in the returned report.
pub fn commit_trash(
    db: &Db,
    trashed: &[(i64, PathBuf)],
    failures: Vec<TrashFailure>,
) -> TrashReport {
    let mut report = TrashReport {
        removed_ids: Vec::new(),
        failures,
    };
    if trashed.is_empty() {
        return report;
    }
    match crate::queries::remove_tracks_matching_paths_remembering_releases(db, trashed) {
        Ok(removed) => {
            for (id, path) in trashed {
                if !removed.contains(id) {
                    report.failures.push(TrashFailure {
                        id: *id,
                        path: path.clone(),
                        error: "file was trashed but its database row was not removed".into(),
                    });
                }
            }
            report.removed_ids = removed;
        }
        Err(error) => {
            for (id, path) in trashed {
                report.failures.push(TrashFailure {
                    id: *id,
                    path: path.clone(),
                    error: format!("file was trashed but database cleanup failed: {error}"),
                });
            }
        }
    }
    report
}

pub fn trash_tracks_with<F>(db: &Db, tracks: &[(i64, PathBuf)], trash_action: F) -> TrashReport
where
    F: Fn(&Path) -> Result<(), String>,
{
    let plan = plan_trash(db, tracks);
    let mut trashed = Vec::new();
    let mut failures = plan.failures;

    for (id, path) in plan.validated {
        match trash_action(&path) {
            Ok(()) => trashed.push((id, path)),
            Err(error) => failures.push(TrashFailure { id, path, error }),
        }
    }

    commit_trash(db, &trashed, failures)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    fn seeded_conn(paths: &[&std::path::Path]) -> Db {
        let conn = Db::open_in_memory().unwrap();
        for (index, path) in paths.iter().enumerate() {
            conn.conn()
                .execute(
                    "INSERT INTO tracks (id,path,title,artist,added_at) VALUES (?1,?2,?3,'',0)",
                    rusqlite::params![
                        index as i64 + 1,
                        path.to_string_lossy().to_string(),
                        format!("Track {}", index + 1)
                    ],
                )
                .unwrap();
        }
        conn
    }

    #[test]
    fn only_successfully_trashed_tracks_are_removed_and_playlists_stay_gapless() {
        let dir = tempfile::tempdir().unwrap();
        let paths: Vec<_> = (1..=3)
            .map(|id| {
                let path = dir.path().join(format!("{id}.flac"));
                std::fs::write(&path, b"scratch").unwrap();
                path
            })
            .collect();
        let refs: Vec<_> = paths.iter().map(std::path::PathBuf::as_path).collect();
        let conn = seeded_conn(&refs);
        let playlist = crate::library::playlists::create(&conn, "Trash").unwrap();
        crate::library::playlists::add_tracks(&conn, playlist, &[1, 2, 3]).unwrap();
        let tracks: Vec<_> = paths
            .iter()
            .enumerate()
            .map(|(index, path)| (index as i64 + 1, path.clone()))
            .collect();

        let report = trash_tracks_with(&conn, &tracks, |path| {
            if path.ends_with("2.flac") {
                Err("injected trash failure".into())
            } else {
                std::fs::remove_file(path).map_err(|error| error.to_string())
            }
        });

        assert_eq!(report.removed_ids, vec![1, 3]);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].id, 2);
        assert!(!paths[0].exists());
        assert!(paths[1].exists());
        assert!(!paths[2].exists());
        let rows: Vec<(i64, i64)> = conn
            .conn()
            .prepare(
                "SELECT track_id,position FROM playlist_tracks \
                 WHERE playlist_id=?1 ORDER BY position",
            )
            .unwrap()
            .query_map([playlist], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(rows, vec![(2, 0)]);
    }

    #[test]
    fn trash_tracks_with_calls_the_action_once_per_validated_path() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first.flac");
        let second = dir.path().join("second.flac");
        let third = dir.path().join("third.flac");
        let conn = seeded_conn(&[&first, &second, &third]);
        let calls = Cell::new(0);
        let tracks = vec![
            (1, first.clone()),
            (1, first),
            (2, second),
            (3, dir.path().join("stale-third.flac")),
        ];

        let report = trash_tracks_with(&conn, &tracks, |_| {
            calls.set(calls.get() + 1);
            Ok(())
        });

        assert_eq!(calls.get(), 2);
        assert_eq!(report.removed_ids, vec![1, 2]);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].id, 3);
    }

    #[test]
    fn nr_32_move_to_trash_writes_deletion_memory_on_completion() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("album.flac");
        std::fs::write(&path, b"scratch").unwrap();
        let db = seeded_conn(&[&path]);
        db.conn()
            .execute(
                "UPDATE tracks
                 SET title = 'Song', artist = 'Artist', album_artist = 'Artist', album = 'Album'
                 WHERE id = 1",
                [],
            )
            .unwrap();
        db.conn()
            .execute(
                "INSERT INTO new_releases (
                   release_group_mbid, artist_name, artist_mbid, title, release_type,
                   first_release_date, fetched_at, first_seen
                 ) VALUES ('release', 'Artist', 'artist-id', 'Album', 'Album',
                           '2026-08-01', 1, 1)",
                [],
            )
            .unwrap();

        let report = trash_tracks_with(&db, &[(1, path.clone())], |target| {
            std::fs::remove_file(target).map_err(|error| error.to_string())
        });

        assert_eq!(report.removed_ids, vec![1]);
        let remembered: i64 = db
            .conn()
            .query_row("SELECT count(*) FROM deleted_releases", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(remembered, 2);
        assert_eq!(crate::artist_news::hidden_release_count(&db).unwrap(), 1);
    }
}
