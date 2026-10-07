//! Platform-independent move-to-trash reconciliation. The caller injects
//! its platform trash action; tests inject scratch-only actions.
//!
//! Trash acts on audio files, and a CUE file holds several tracks: it goes to
//! the trash only when every track of it still in the library is selected,
//! together with the sheet beside it once no other file still needs that
//! sheet. The selected tracks of a file that is only partly selected are
//! hidden instead, as Remove from Library hides them (CUE-11).

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
    /// Every track that left the library: trashed, or hidden.
    pub removed_ids: Vec<i64>,
    /// The tracks of partly selected CUE files that were hidden, not trashed.
    pub hidden_ids: Vec<i64>,
    pub failures: Vec<TrashFailure>,
}

/// One audio file the selection covers completely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashFile {
    pub path: PathBuf,
    /// Every track of the file, as selected.
    pub tracks: Vec<(i64, PathBuf)>,
    /// The sheet beside the file that cut it, to be trashed after the file:
    /// set on the last file of the selection that the sheet describes, and
    /// only when no file outside the selection still needs it.
    pub sheet: Option<PathBuf>,
}

/// A selection grouped by audio file.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FileTrashPlan {
    pub files: Vec<TrashFile>,
    /// Selected tracks of CUE files whose other tracks stay; they are hidden.
    pub hidden: Vec<(i64, PathBuf)>,
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
        failures,
        ..TrashReport::default()
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

/// Groups the validated selection by file: a file whose every present track
/// is selected is trashed whole, the selected tracks of any other file are
/// hidden. See [`TrashFile::sheet`] for when a sheet goes along.
pub fn plan_file_trash(db: &Db, tracks: &[(i64, PathBuf)]) -> FileTrashPlan {
    let validated = plan_trash(db, tracks);
    let mut plan = FileTrashPlan {
        failures: validated.failures,
        ..FileTrashPlan::default()
    };
    let mut groups: Vec<(PathBuf, Vec<(i64, PathBuf)>)> = Vec::new();
    for (id, path) in validated.validated {
        match groups.iter_mut().find(|(file, _)| *file == path) {
            Some((_, group)) => group.push((id, path)),
            None => groups.push((path.clone(), vec![(id, path)])),
        }
    }
    let conn = db.conn();
    let mut sheets = Vec::new();
    for (path, group) in groups {
        match file_layout(conn, &path) {
            Ok(layout)
                if layout
                    .present_ids
                    .iter()
                    .all(|id| group.iter().any(|(selected, _)| selected == id)) =>
            {
                sheets.push(layout.sheet);
                plan.files.push(TrashFile {
                    path,
                    tracks: group,
                    sheet: None,
                });
            }
            Ok(_) => plan.hidden.extend(group),
            Err(error) => plan
                .failures
                .extend(group.into_iter().map(|(id, path)| TrashFailure {
                    id,
                    path,
                    error: format!("could not read the tracks of the file before trash: {error}"),
                })),
        }
    }
    assign_sheets(conn, &mut plan.files, &sheets);
    plan
}

/// What the catalog holds for one audio file.
struct FileLayout {
    /// The tracks still in the library.
    present_ids: Vec<i64>,
    /// The sheet beside the file that cut it into tracks.
    sheet: Option<PathBuf>,
}

fn file_layout(conn: &rusqlite::Connection, path: &Path) -> Result<FileLayout, rusqlite::Error> {
    let path = path.to_string_lossy();
    let present_ids = conn
        .prepare_cached(&format!(
            "SELECT id FROM tracks WHERE path = ?1 AND {}",
            crate::queries::PRESENT
        ))?
        .query_map([&path], |row| row.get(0))?
        .collect::<Result<Vec<i64>, _>>()?;
    // A whole-file row may remember a sheet that did not fit it; only a sheet
    // the file's tracks remember is the file's: the one that cut it, or one
    // that did not fit and gave way to the sheet embedded in the file.
    let sheet = conn
        .prepare_cached("SELECT min(cue_path) FROM tracks WHERE path = ?1 AND segment_index > 0")?
        .query_row([&path], |row| row.get::<_, Option<String>>(0))?
        .map(PathBuf::from);
    Ok(FileLayout { present_ids, sheet })
}

/// Hands each sheet to the last file of the selection it describes, unless a
/// file outside the selection, in the library or hidden from it, still names
/// it: without its sheet a hidden CUE file would come back whole.
fn assign_sheets(conn: &rusqlite::Connection, files: &mut [TrashFile], sheets: &[Option<PathBuf>]) {
    let trashed: HashSet<String> = files
        .iter()
        .map(|file| file.path.to_string_lossy().into_owned())
        .collect();
    for (position, sheet) in sheets.iter().enumerate() {
        let Some(sheet) = sheet else { continue };
        let later = sheets[position + 1..]
            .iter()
            .any(|other| other.as_ref() == Some(sheet));
        if later {
            continue;
        }
        match paths_naming_sheet(conn, sheet) {
            Ok(paths) if paths.iter().all(|path| trashed.contains(path)) => {
                files[position].sheet = Some(sheet.clone());
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(sheet = %sheet.display(), %error, "kept a CUE sheet: could not tell which files still need it");
            }
        }
    }
}

fn paths_naming_sheet(
    conn: &rusqlite::Connection,
    sheet: &Path,
) -> Result<Vec<String>, rusqlite::Error> {
    let sheet = sheet.to_string_lossy();
    let mut paths = conn
        .prepare_cached("SELECT DISTINCT path FROM tracks WHERE cue_path = ?1")?
        .query_map([&sheet], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    paths.extend(crate::db_library_exclusions::paths_naming_sheet(
        conn, &sheet,
    )?);
    Ok(paths)
}

/// Trashes the selection per file (see [`plan_file_trash`]) and hides the
/// selected tracks of files that are only partly selected.
pub fn trash_tracks_with<F>(db: &Db, tracks: &[(i64, PathBuf)], trash_action: F) -> TrashReport
where
    F: Fn(&Path) -> Result<(), String>,
{
    let plan = plan_file_trash(db, tracks);
    let mut trashed = Vec::new();
    let mut failures = plan.failures;

    for file in plan.files {
        match trash_action(&file.path) {
            Ok(()) => {
                trashed.extend(file.tracks);
                if let Some(sheet) = file.sheet {
                    // The audio is gone and its rows go with it; a sheet left
                    // behind only raises an issue on the next scan.
                    if let Err(error) = trash_action(&sheet) {
                        tracing::warn!(sheet = %sheet.display(), %error, "move-to-trash of a CUE sheet failed");
                    }
                }
            }
            Err(error) => failures.extend(file.tracks.into_iter().map(|(id, path)| TrashFailure {
                id,
                path,
                error: error.clone(),
            })),
        }
    }

    let mut report = commit_trash(db, &trashed, failures);
    hide_partial_selection(db, &plan.hidden, &mut report);
    report
}

fn hide_partial_selection(db: &Db, hidden: &[(i64, PathBuf)], report: &mut TrashReport) {
    if hidden.is_empty() {
        return;
    }
    let excluded_at = crate::library::stats::now_unix();
    match crate::queries::exclude_tracks_matching_paths(db, hidden, excluded_at) {
        Ok(ids) => {
            for (id, path) in hidden {
                if !ids.contains(id) {
                    report.failures.push(TrashFailure {
                        id: *id,
                        path: path.clone(),
                        error: "track changed before it could be hidden".into(),
                    });
                }
            }
            report.removed_ids.extend(&ids);
            report.hidden_ids = ids;
        }
        Err(error) => report
            .failures
            .extend(hidden.iter().map(|(id, path)| TrashFailure {
                id: *id,
                path: path.clone(),
                error: format!("could not hide the track: {error}"),
            })),
    }
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

#[cfg(test)]
#[path = "trash_tracks_cue_tests.rs"]
mod cue_tests;
