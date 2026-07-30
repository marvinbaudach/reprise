use super::*;
use crate::dto::PlayParams;

/// Seeds one real track row via the actual scanner (`reprise_core::
/// library::scanner::scan_folder`) over a temp copy of the shared
/// `sine.flac` fixture, then reads its assigned id back through the
/// existing `queries::query_track_window` facade — never a raw SQL
/// literal. `resolve_play_ids`'s playlist path needs a track that is
/// genuinely present in `tracks` (the `playlist_tracks.track_id` foreign
/// key is enforced, per `db::open`'s `PRAGMA foreign_keys = ON`), and
/// `scripts/check-architecture.sh`'s "no SQL outside reprise-core" gate
/// scans all of `crates/reprise-mcp/src` verbatim — `#[cfg(test)]` blocks
/// included, unlike the `tests/` integration fixtures it explicitly
/// exempts — so a hand-written literal `tracks`-table insert here would
/// trip it even though it never ships in the binary.
fn scan_one_track(db_path: &Path) -> i64 {
    let db = reprise_core::db::Db::open_migrated(Some(db_path)).unwrap();

    let library_root = tempfile::tempdir().unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    std::fs::copy(&fixture, library_root.path().join("sine.flac")).unwrap();
    reprise_core::library::scanner::scan_folder(&db, library_root.path()).unwrap();

    let source = ViewSource::Library;
    let tracks = queries::query_track_window(&db, &source, "title", "asc", "", 0, 10, &[]).unwrap();
    assert_eq!(tracks.len(), 1, "expected exactly one scanned track");
    tracks[0].id
}

#[test]
fn resolve_play_ids_enforces_exactly_one_source() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    let track_id = scan_one_track(&path);

    let db = reprise_core::db::Db::open_migrated(Some(&path)).unwrap();
    let pid = playlists::create(&db, "P").unwrap();
    playlists::add_tracks(&db, pid, &[track_id]).unwrap();
    drop(db);

    // playlist path
    let ids = resolve_play_ids(
        &path,
        &PlayParams {
            track_ids: None,
            playlist_id: Some(pid),
        },
    )
    .unwrap();
    assert_eq!(ids, vec![track_id]);
    // explicit ids path
    let ids = resolve_play_ids(
        &path,
        &PlayParams {
            track_ids: Some(vec![track_id]),
            playlist_id: None,
        },
    )
    .unwrap();
    assert_eq!(ids, vec![track_id]);
    // neither
    assert!(matches!(
        resolve_play_ids(
            &path,
            &PlayParams {
                track_ids: None,
                playlist_id: None,
            }
        ),
        Err(DataError::InvalidInput(_))
    ));
    // both
    assert!(matches!(
        resolve_play_ids(
            &path,
            &PlayParams {
                track_ids: Some(vec![track_id]),
                playlist_id: Some(pid),
            }
        ),
        Err(DataError::InvalidInput(_))
    ));
    // empty playlist
    let db = reprise_core::db::Db::open_migrated(Some(&path)).unwrap();
    let empty = playlists::create(&db, "E").unwrap();
    assert!(matches!(
        resolve_play_ids(
            &path,
            &PlayParams {
                track_ids: None,
                playlist_id: Some(empty),
            }
        ),
        Err(DataError::InvalidInput(_))
    ));
}
