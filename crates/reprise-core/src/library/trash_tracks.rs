//! Platform-independent move-to-trash reconciliation. The caller injects
//! its platform trash action; tests inject scratch-only actions.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension};

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

pub fn trash_tracks_with<F>(
    conn: &mut Connection,
    tracks: &[(i64, PathBuf)],
    trash_action: F,
) -> TrashReport
where
    F: Fn(&Path) -> Result<(), String>,
{
    let mut report = TrashReport::default();
    let mut trashed = Vec::new();
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
            Ok(Some(registered)) if registered == path.to_string_lossy() => {}
            Ok(_) => {
                report.failures.push(TrashFailure {
                    id: *id,
                    path: path.clone(),
                    error: "track path changed before trash; refusing stale request".into(),
                });
                continue;
            }
            Err(error) => {
                report.failures.push(TrashFailure {
                    id: *id,
                    path: path.clone(),
                    error: format!("could not validate track path before trash: {error}"),
                });
                continue;
            }
        }

        match trash_action(path) {
            Ok(()) => trashed.push((*id, path.clone())),
            Err(error) => report.failures.push(TrashFailure {
                id: *id,
                path: path.clone(),
                error,
            }),
        }
    }

    if trashed.is_empty() {
        return report;
    }
    let ids: Vec<i64> = trashed.iter().map(|(id, _)| *id).collect();
    match crate::queries::remove_tracks(conn, &ids) {
        Ok(removed) => {
            for (id, path) in trashed {
                if !removed.contains(&id) {
                    report.failures.push(TrashFailure {
                        id,
                        path,
                        error: "file was trashed but its database row was not removed".into(),
                    });
                }
            }
            report.removed_ids = removed;
        }
        Err(error) => {
            for (id, path) in trashed {
                report.failures.push(TrashFailure {
                    id,
                    path,
                    error: format!("file was trashed but database cleanup failed: {error}"),
                });
            }
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded_conn(paths: &[&std::path::Path]) -> rusqlite::Connection {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        for (index, path) in paths.iter().enumerate() {
            conn.execute(
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
        let mut conn = seeded_conn(&refs);
        let playlist = crate::library::playlists::create(&conn, "Trash").unwrap();
        crate::library::playlists::add_tracks(&mut conn, playlist, &[1, 2, 3]).unwrap();
        let tracks: Vec<_> = paths
            .iter()
            .enumerate()
            .map(|(index, path)| (index as i64 + 1, path.clone()))
            .collect();

        let report = trash_tracks_with(&mut conn, &tracks, |path| {
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
}
