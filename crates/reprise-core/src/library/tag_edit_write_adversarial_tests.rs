use std::path::{Path, PathBuf};

use lofty::prelude::*;
use rusqlite::Connection;

use super::tag_edit::{read_editable_tags, TagPatch, TrackEditPatch};
use super::tag_edit_write::{apply_track_writes, TrackWrite};

fn fixture_copy(dir: &Path, name: &str) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    let destination = dir.join(name);
    std::fs::copy(source, &destination).unwrap();
    destination
}

fn seed_title(path: &Path) {
    let mut tagged = lofty::read_from_path(path).unwrap();
    tagged
        .primary_tag_mut()
        .unwrap()
        .set_title("Old title".into());
    tagged
        .primary_tag()
        .unwrap()
        .save_to_path(path, lofty::config::WriteOptions::default())
        .unwrap();
}

fn seeded_track(dir: &Path, name: &str) -> (Connection, i64, PathBuf) {
    let path = fixture_copy(dir, name);
    seed_title(&path);
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    crate::library::scanner::scan_folder(&mut conn, &path).unwrap();
    let id = conn
        .query_row(
            "SELECT id FROM tracks WHERE path=?1",
            [path.to_string_lossy().as_ref()],
            |row| row.get(0),
        )
        .unwrap();
    (conn, id, path)
}

#[test]
fn out_of_range_year_fails_before_file_or_journal_write() {
    let dir = tempfile::tempdir().unwrap();
    let (mut conn, id, path) = seeded_track(dir.path(), "bad-year.flac");
    let bytes = std::fs::read(&path).unwrap();
    let report = apply_track_writes(
        &mut conn,
        &[TrackWrite {
            id,
            path: path.clone(),
            patch: TrackEditPatch {
                tags: TagPatch {
                    year: Some(Some(70_000)),
                    ..TagPatch::default()
                },
                rating: None,
            },
        }],
        &mut |_, _| {},
    );
    assert_eq!(report.failures.len(), 1);
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    let jobs: i64 = conn
        .query_row("SELECT COUNT(*) FROM tag_write_jobs", [], |row| row.get(0))
        .unwrap();
    assert_eq!(jobs, 0);
}

#[test]
fn duplicate_requests_fail_only_duplicates_and_keep_unrelated_write() {
    let dir = tempfile::tempdir().unwrap();
    let (mut conn, duplicate_id, duplicate_path) = seeded_track(dir.path(), "duplicate.flac");
    let unique_path = fixture_copy(dir.path(), "unique.flac");
    seed_title(&unique_path);
    crate::library::scanner::scan_folder(&mut conn, &unique_path).unwrap();
    let unique_id = conn
        .query_row(
            "SELECT id FROM tracks WHERE path=?1",
            [unique_path.to_string_lossy().as_ref()],
            |row| row.get(0),
        )
        .unwrap();
    let tag_write = |id, path: PathBuf, title: &str| TrackWrite {
        id,
        path,
        patch: TrackEditPatch {
            tags: TagPatch {
                title: Some(title.into()),
                ..TagPatch::default()
            },
            rating: None,
        },
    };
    let writes = [
        tag_write(duplicate_id, duplicate_path.clone(), "Duplicate one"),
        tag_write(duplicate_id, duplicate_path.clone(), "Duplicate two"),
        tag_write(unique_id, unique_path.clone(), "Unique write"),
    ];
    let report = apply_track_writes(&mut conn, &writes, &mut |_, _| {});
    assert_eq!(report.failures.len(), 2);
    assert_eq!(report.updated_ids, vec![unique_id]);
    assert_eq!(
        read_editable_tags(&duplicate_path).unwrap().title,
        "Old title"
    );
    assert_eq!(
        read_editable_tags(&unique_path).unwrap().title,
        "Unique write"
    );
    let journal_tracks: i64 = conn
        .query_row("SELECT total_tracks FROM tag_write_jobs", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(journal_tracks, 1);
}

#[test]
fn rating_revalidates_id_path_after_progress_callback() {
    let dir = tempfile::tempdir().unwrap();
    let first_path = fixture_copy(dir.path(), "rating-first.flac");
    let second_path = fixture_copy(dir.path(), "rating-second.flac");
    seed_title(&first_path);
    seed_title(&second_path);
    let database = dir.path().join("ratings.db");
    let mut conn = crate::db::open_migrated(Some(&database)).unwrap();
    crate::library::scanner::scan_folder(&mut conn, &first_path).unwrap();
    crate::library::scanner::scan_folder(&mut conn, &second_path).unwrap();
    let id_for = |path: &Path| {
        conn.query_row(
            "SELECT id FROM tracks WHERE path=?1",
            [path.to_string_lossy().as_ref()],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
    };
    let first_id = id_for(&first_path);
    let second_id = id_for(&second_path);
    let other = crate::db::open_migrated(Some(&database)).unwrap();
    let replacement = dir.path().join("moved.flac");
    let writes = [(first_id, first_path), (second_id, second_path)].map(|(id, path)| TrackWrite {
        id,
        path,
        patch: TrackEditPatch {
            tags: TagPatch::default(),
            rating: Some(4),
        },
    });
    let report = apply_track_writes(&mut conn, &writes, &mut |done, _| {
        if done == 1 {
            other
                .execute(
                    "UPDATE tracks SET path=?1 WHERE id=?2",
                    rusqlite::params![replacement.to_string_lossy(), second_id],
                )
                .unwrap();
        }
    });
    assert_eq!(report.updated_ids, vec![first_id]);
    assert_eq!(report.failures.len(), 1);
    let second_rating: i32 = conn
        .query_row(
            "SELECT rating FROM tracks WHERE id=?1",
            [second_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(second_rating, 0);
}
