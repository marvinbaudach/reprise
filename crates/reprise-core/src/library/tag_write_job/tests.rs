use std::path::{Path, PathBuf};

use lofty::prelude::*;
use rusqlite::Connection;

use super::{
    execute_tag_write_file, prepare_tag_write_job, recover_incomplete_tag_write_jobs,
    RecoveryState, TagWriteJobSpec,
};
use crate::library::tag_edit::{read_editable_tags, TagPatch};
use crate::library::tag_mutation::{apply_tag_patch_to_file, prepare_tag_mutation};

fn fixture_copy(dir: &Path, name: &str) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    let destination = dir.join(name);
    std::fs::copy(source, &destination).unwrap();
    destination
}

fn seeded_track(dir: &Path, name: &str) -> (Connection, i64, PathBuf) {
    let path = fixture_copy(dir, name);
    let mut tagged = lofty::read_from_path(&path).unwrap();
    tagged
        .primary_tag_mut()
        .unwrap()
        .set_title("Before title".into());
    tagged
        .primary_tag_mut()
        .unwrap()
        .set_date(lofty::tag::items::Timestamp {
            year: 1999,
            ..lofty::tag::items::Timestamp::default()
        });
    tagged.primary_tag_mut().unwrap().set_track(7);
    tagged
        .primary_tag()
        .unwrap()
        .save_to_path(&path, lofty::config::WriteOptions::default())
        .unwrap();
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

fn prepared_title_job(
    conn: &mut Connection,
    id: i64,
    path: &Path,
) -> super::types::PreparedTagWriteJob {
    let mutation = prepare_tag_mutation(
        conn,
        id,
        path,
        &TagPatch {
            title: Some("After title".into()),
            ..TagPatch::default()
        },
    )
    .unwrap()
    .unwrap();
    prepare_tag_write_job(conn, TagWriteJobSpec::tag_editor(), &[(0, mutation)]).unwrap()
}

#[test]
fn journal_prepare_is_durable_before_any_file_write() {
    let dir = tempfile::tempdir().unwrap();
    let (mut conn, id, path) = seeded_track(dir.path(), "prepared.flac");

    let job = prepared_title_job(&mut conn, id, &path);

    assert_eq!(read_editable_tags(&path).unwrap().title, "Before title");
    let row: (String, String, String, i64, i64) = conn
        .query_row(
            "SELECT j.kind, f.state, v.before_value, v.before_is_null, v.after_is_null \
             FROM tag_write_jobs j \
             JOIN tag_write_job_files f ON f.job_id=j.id \
             JOIN tag_write_journal v ON v.file_id=f.id \
             WHERE j.id=?1 AND v.field='title'",
            [job.id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        row,
        (
            "tag_editor".into(),
            "pending".into(),
            "Before title".into(),
            0,
            0
        )
    );
    let after: String = conn
        .query_row(
            "SELECT after_value FROM tag_write_journal v \
             JOIN tag_write_job_files f ON f.id=v.file_id WHERE f.job_id=?1",
            [job.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(after, "After title");
}

#[test]
fn crash_recovery_classifies_without_writing_files() {
    let dir = tempfile::tempdir().unwrap();

    let (mut before_conn, before_id, before_path) = seeded_track(dir.path(), "not-applied.flac");
    prepared_title_job(&mut before_conn, before_id, &before_path);
    let before_bytes = std::fs::read(&before_path).unwrap();
    let changes_before_recovery = before_conn.total_changes();
    let before_recovery = recover_incomplete_tag_write_jobs(&before_conn).unwrap();
    assert_eq!(before_recovery[0].state, RecoveryState::NotApplied);
    assert_eq!(std::fs::read(&before_path).unwrap(), before_bytes);
    assert_eq!(before_conn.total_changes(), changes_before_recovery);

    let (mut applied_conn, applied_id, applied_path) = seeded_track(dir.path(), "applied.flac");
    let applied_job = prepared_title_job(&mut applied_conn, applied_id, &applied_path);
    execute_tag_write_file(
        &mut applied_conn,
        applied_job.id,
        &applied_job.files[0],
        false,
        &mut |_, _, _| {},
    )
    .unwrap();
    let applied_bytes = std::fs::read(&applied_path).unwrap();
    let applied_recovery = recover_incomplete_tag_write_jobs(&applied_conn).unwrap();
    assert!(applied_recovery.is_empty());
    assert_eq!(std::fs::read(&applied_path).unwrap(), applied_bytes);

    let (mut conflict_conn, conflict_id, conflict_path) = seeded_track(dir.path(), "conflict.flac");
    prepared_title_job(&mut conflict_conn, conflict_id, &conflict_path);
    apply_tag_patch_to_file(
        &conflict_path,
        &TagPatch {
            title: Some("External title".into()),
            ..TagPatch::default()
        },
    )
    .unwrap();
    let conflict_recovery = recover_incomplete_tag_write_jobs(&conflict_conn).unwrap();
    assert_eq!(conflict_recovery[0].state, RecoveryState::Conflict);

    let (mut missing_conn, missing_id, missing_path) = seeded_track(dir.path(), "missing.flac");
    prepared_title_job(&mut missing_conn, missing_id, &missing_path);
    std::fs::remove_file(&missing_path).unwrap();
    let missing_recovery = recover_incomplete_tag_write_jobs(&missing_conn).unwrap();
    assert_eq!(missing_recovery[0].state, RecoveryState::Unavailable);

    let (mut unreadable_conn, unreadable_id, unreadable_path) =
        seeded_track(dir.path(), "unreadable.flac");
    prepared_title_job(&mut unreadable_conn, unreadable_id, &unreadable_path);
    std::fs::write(&unreadable_path, b"not an audio container").unwrap();
    let unreadable_recovery = recover_incomplete_tag_write_jobs(&unreadable_conn).unwrap();
    assert_eq!(unreadable_recovery[0].state, RecoveryState::Unavailable);
}

#[test]
fn journal_preserves_present_and_absent_numeric_values() {
    let dir = tempfile::tempdir().unwrap();
    let (mut conn, id, path) = seeded_track(dir.path(), "numbers.flac");
    let mutation = prepare_tag_mutation(
        &conn,
        id,
        &path,
        &TagPatch {
            year: Some(None),
            track_no: Some(Some(9)),
            ..TagPatch::default()
        },
    )
    .unwrap()
    .unwrap();

    let job =
        prepare_tag_write_job(&mut conn, TagWriteJobSpec::tag_editor(), &[(0, mutation)]).unwrap();
    let rows = {
        let mut statement = conn
            .prepare(
                "SELECT field, before_value, before_is_null, after_value, after_is_null \
                 FROM tag_write_journal v \
                 JOIN tag_write_job_files f ON f.id=v.file_id \
                 WHERE f.job_id=?1 ORDER BY field",
            )
            .unwrap();
        statement
            .query_map([job.id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };

    assert_eq!(
        rows,
        vec![
            ("track_no".into(), Some("7".into()), 0, Some("9".into()), 0),
            ("year".into(), Some("1999".into()), 0, None, 1),
        ]
    );
}

#[test]
fn status_update_failure_leaves_after_file_recoverable_and_job_interrupted() {
    let dir = tempfile::tempdir().unwrap();
    let (mut conn, id, path) = seeded_track(dir.path(), "status-failure.flac");
    let job = prepared_title_job(&mut conn, id, &path);
    conn.execute_batch(
        "CREATE TRIGGER reject_file_completion
         BEFORE UPDATE OF state ON tag_write_job_files
         WHEN NEW.state='complete'
         BEGIN
           SELECT RAISE(FAIL, 'injected status failure');
         END;",
    )
    .unwrap();

    let failure =
        execute_tag_write_file(&mut conn, job.id, &job.files[0], false, &mut |_, _, _| {})
            .unwrap_err();
    assert!(failure.file_written);
    super::finish_tag_write_job(&conn, job.id).unwrap();

    let persisted: (String, String, String) = conn
        .query_row(
            "SELECT j.state, f.state, v.outcome \
             FROM tag_write_jobs j \
             JOIN tag_write_job_files f ON f.job_id=j.id \
             JOIN tag_write_journal v ON v.file_id=f.id \
             WHERE j.id=?1",
            [job.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        persisted,
        ("interrupted".into(), "running".into(), "prepared".into())
    );
    let changes = conn.total_changes();
    let recovery = recover_incomplete_tag_write_jobs(&conn).unwrap();
    assert_eq!(recovery[0].state, RecoveryState::Applied);
    assert_eq!(conn.total_changes(), changes);
}

#[test]
fn file_only_after_state_survives_close_and_reopens_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let path = fixture_copy(dir.path(), "reopen.flac");
    let mut tagged = lofty::read_from_path(&path).unwrap();
    tagged
        .primary_tag_mut()
        .unwrap()
        .set_title("Before title".into());
    tagged
        .primary_tag()
        .unwrap()
        .save_to_path(&path, lofty::config::WriteOptions::default())
        .unwrap();
    let database = dir.path().join("recovery.db");
    let mut conn = crate::db::open_migrated(Some(&database)).unwrap();
    crate::library::scanner::scan_folder(&mut conn, &path).unwrap();
    let id = conn
        .query_row(
            "SELECT id FROM tracks WHERE path=?1",
            [path.to_string_lossy().as_ref()],
            |row| row.get(0),
        )
        .unwrap();
    prepared_title_job(&mut conn, id, &path);
    apply_tag_patch_to_file(
        &path,
        &TagPatch {
            title: Some("After title".into()),
            ..TagPatch::default()
        },
    )
    .unwrap();
    drop(conn);

    let reopened = crate::db::open_migrated(Some(&database)).unwrap();
    let changes = reopened.total_changes();
    let recovery = recover_incomplete_tag_write_jobs(&reopened).unwrap();
    assert_eq!(recovery[0].state, RecoveryState::Applied);
    assert_eq!(reopened.total_changes(), changes);
}

#[test]
fn empty_field_set_recovers_as_conflict_not_vacuous_applied() {
    let dir = tempfile::tempdir().unwrap();
    let (conn, id, path) = seeded_track(dir.path(), "empty-fields.flac");
    conn.execute(
        "INSERT INTO tag_write_jobs \
         (kind, state, created_at, finished_at, total_tracks) \
         VALUES ('tag_editor', 'interrupted', 0, 1, 1)",
        [],
    )
    .unwrap();
    let job_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO tag_write_job_files \
         (job_id, position, track_id, path, state, file_written) \
         VALUES (?1, 0, ?2, ?3, 'running', 0)",
        rusqlite::params![job_id, id, path.to_string_lossy()],
    )
    .unwrap();

    let recovery = recover_incomplete_tag_write_jobs(&conn).unwrap();
    assert_eq!(recovery[0].state, RecoveryState::Conflict);
}

#[test]
fn interrupted_recovery_ignores_terminal_siblings_even_if_they_change_later() {
    let dir = tempfile::tempdir().unwrap();
    let (mut conn, first_id, first_path) = seeded_track(dir.path(), "terminal.flac");
    let second_path = fixture_copy(dir.path(), "uncertain.flac");
    let mut second_tagged = lofty::read_from_path(&second_path).unwrap();
    second_tagged
        .primary_tag_mut()
        .unwrap()
        .set_title("Before title".into());
    second_tagged
        .primary_tag()
        .unwrap()
        .save_to_path(&second_path, lofty::config::WriteOptions::default())
        .unwrap();
    crate::library::scanner::scan_folder(&mut conn, &second_path).unwrap();
    let second_id = conn
        .query_row(
            "SELECT id FROM tracks WHERE path=?1",
            [second_path.to_string_lossy().as_ref()],
            |row| row.get(0),
        )
        .unwrap();
    let patch = TagPatch {
        title: Some("After title".into()),
        ..TagPatch::default()
    };
    let first = prepare_tag_mutation(&conn, first_id, &first_path, &patch)
        .unwrap()
        .unwrap();
    let second = prepare_tag_mutation(&conn, second_id, &second_path, &patch)
        .unwrap()
        .unwrap();
    let job = prepare_tag_write_job(
        &mut conn,
        TagWriteJobSpec::tag_editor(),
        &[(0, first), (1, second)],
    )
    .unwrap();
    execute_tag_write_file(&mut conn, job.id, &job.files[0], false, &mut |_, _, _| {}).unwrap();
    super::finish_tag_write_job(&conn, job.id).unwrap();
    apply_tag_patch_to_file(
        &first_path,
        &TagPatch {
            title: Some("External later edit".into()),
            ..TagPatch::default()
        },
    )
    .unwrap();

    let recovery = recover_incomplete_tag_write_jobs(&conn).unwrap();

    assert_eq!(recovery.len(), 1);
    assert_eq!(recovery[0].track_id, second_id);
    assert_eq!(recovery[0].state, RecoveryState::NotApplied);
}
