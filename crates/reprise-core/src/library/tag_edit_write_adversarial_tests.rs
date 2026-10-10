use std::path::{Path, PathBuf};

use lofty::prelude::*;

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

fn seeded_track(dir: &Path, name: &str) -> (crate::db::Db, i64, PathBuf) {
    let path = fixture_copy(dir, name);
    seed_title(&path);
    let conn = crate::db::Db::open_in_memory().unwrap();
    crate::library::scanner::scan_folder(&conn, &path).unwrap();
    let id = conn
        .conn()
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
    let (conn, id, path) = seeded_track(dir.path(), "bad-year.flac");
    let bytes = std::fs::read(&path).unwrap();
    let report = apply_track_writes(
        &conn,
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
        .conn()
        .query_row("SELECT COUNT(*) FROM tag_write_jobs", [], |row| row.get(0))
        .unwrap();
    assert_eq!(jobs, 0);
}

#[test]
fn duplicate_requests_fail_only_duplicates_and_keep_unrelated_write() {
    let dir = tempfile::tempdir().unwrap();
    let (conn, duplicate_id, duplicate_path) = seeded_track(dir.path(), "duplicate.flac");
    let unique_path = fixture_copy(dir.path(), "unique.flac");
    seed_title(&unique_path);
    crate::library::scanner::scan_folder(&conn, &unique_path).unwrap();
    let unique_id = conn
        .conn()
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
    let report = apply_track_writes(&conn, &writes, &mut |_, _| {});
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
        .conn()
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
    let conn = crate::db::Db::open_migrated(Some(&database)).unwrap();
    crate::library::scanner::scan_folder(&conn, &first_path).unwrap();
    crate::library::scanner::scan_folder(&conn, &second_path).unwrap();
    let id_for = |path: &Path| {
        conn.conn()
            .query_row(
                "SELECT id FROM tracks WHERE path=?1",
                [path.to_string_lossy().as_ref()],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
    };
    let first_id = id_for(&first_path);
    let second_id = id_for(&second_path);
    let other = crate::db::Db::open_ready(&database).unwrap();
    let replacement = dir.path().join("moved.flac");
    let writes = [(first_id, first_path), (second_id, second_path)].map(|(id, path)| TrackWrite {
        id,
        path,
        patch: TrackEditPatch {
            tags: TagPatch::default(),
            rating: Some(4),
        },
    });
    let report = apply_track_writes(&conn, &writes, &mut |done, _| {
        if done == 1 {
            other
                .conn()
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
        .conn()
        .query_row(
            "SELECT rating FROM tracks WHERE id=?1",
            [second_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(second_rating, 0);
}

#[test]
fn a_claimed_file_without_a_result_is_failed_through_completion() {
    use super::tag_edit_write_pipeline::settle_file_result;
    use super::tag_mutation::{prepare_tag_mutation, WriteErrorKind};
    use super::tag_write_job::{begin_tag_write_file, prepare_tag_write_job, TagWriteJobSpec};

    let dir = tempfile::tempdir().unwrap();
    let (db, id, path) = seeded_track(dir.path(), "no-result.flac");
    let patch = TagPatch {
        title: Some("New title".into()),
        ..TagPatch::default()
    };
    let mutation = prepare_tag_mutation(db.conn(), id, &path, &patch)
        .unwrap()
        .expect("the title differs, so there is a mutation");
    let job =
        prepare_tag_write_job(db.conn(), TagWriteJobSpec::tag_editor(), &[(0, mutation)]).unwrap();
    let file = job.files.first().expect("one journaled file");
    begin_tag_write_file(db.conn(), job.id, file, &mut |_, _, _| {}).unwrap();

    let failure = settle_file_result(db.conn(), file, None, true).unwrap_err();

    assert_eq!(failure.kind, WriteErrorKind::Io);
    assert!(!failure.file_written);
    let (state, outcomes): (String, i64) = db
        .conn()
        .query_row(
            "SELECT f.state, \
               (SELECT COUNT(*) FROM tag_write_journal WHERE file_id=f.id AND outcome='failed') \
             FROM tag_write_job_files f WHERE f.id=?1",
            [file.file_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(state, "failed", "the claimed row must not stay running");
    assert_eq!(outcomes, 1);
}
