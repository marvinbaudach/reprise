use super::write_slot::tag_write_slot_status_for_liveness;
use super::{LibraryDoctor, TagWriteJobKind, TagWriteSlotOwner, TagWriteSlotStatus};
use crate::library::{TagWriteLiveness, TagWriteLock};

fn seed_partial_job(db: &crate::db::Db) {
    db.conn()
        .execute(
            "INSERT INTO library_doctor_scans \
             (id, scope_kind, created_at, remote_enabled, checked_tracks, skipped_tracks) \
             VALUES (1, 'selection', 1724964804, 0, 2, 0)",
            [],
        )
        .unwrap();
    db.conn()
        .execute(
            "INSERT INTO tag_write_jobs \
             (id, kind, scan_id, state, created_at, total_tracks) \
             VALUES (19, 'doctor_apply', 1, 'running', 1724964804, 2)",
            [],
        )
        .unwrap();
    for (position, state, written, outcome) in
        [(0, "complete", 1, "applied"), (1, "pending", 0, "pending")]
    {
        db.conn()
            .execute(
                "INSERT INTO tag_write_job_files \
                 (job_id, position, track_id, path, state, file_written) \
                 VALUES (19, ?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    position,
                    position + 1,
                    format!("track-{position}.flac"),
                    state,
                    written
                ],
            )
            .unwrap();
        let file_id = db.conn().last_insert_rowid();
        db.conn()
            .execute(
                "INSERT INTO tag_write_journal \
                 (file_id, position, field, guard_is_set, expected_is_null, before_value, \
                  before_is_null, after_value, after_is_null, outcome) \
                 VALUES (?1, 0, 'title', 0, 1, 'Before', 0, 'After', 0, ?2)",
                rusqlite::params![file_id, outcome],
            )
            .unwrap();
    }
}

#[test]
fn slot_status_reports_free_busy_and_orphaned_with_partial_progress() {
    let dir = tempfile::tempdir().unwrap();
    let db = crate::db::Db::open_migrated(Some(&dir.path().join("reprise.db"))).unwrap();
    assert_eq!(
        LibraryDoctor::new(&db)
            .tag_write_slot_status(dir.path())
            .unwrap(),
        TagWriteSlotStatus::Free
    );
    seed_partial_job(&db);
    let held = TagWriteLock::acquire(dir.path()).unwrap();

    assert_eq!(
        LibraryDoctor::new(&db)
            .tag_write_slot_status(dir.path())
            .unwrap(),
        TagWriteSlotStatus::Busy(TagWriteSlotOwner {
            job_id: 19,
            kind: TagWriteJobKind::DoctorApply,
            completed_tracks: 1,
            total_tracks: 2,
            created_at: 1_724_964_804,
        })
    );

    drop(held);
    assert!(matches!(
        LibraryDoctor::new(&db)
            .tag_write_slot_status(dir.path())
            .unwrap(),
        TagWriteSlotStatus::Orphaned(TagWriteSlotOwner { job_id: 19, .. })
    ));
    assert_eq!(
        LibraryDoctor::new(&db)
            .tag_write_slot_status(dir.path())
            .unwrap(),
        TagWriteSlotStatus::Free
    );
}

#[test]
fn unknown_liveness_is_busy_never_orphaned() {
    let owner = TagWriteSlotOwner {
        job_id: 19,
        kind: TagWriteJobKind::TagEditor,
        completed_tracks: 18,
        total_tracks: 275,
        created_at: 1_724_964_804,
    };

    assert_eq!(
        tag_write_slot_status_for_liveness(owner.clone(), TagWriteLiveness::Unknown),
        TagWriteSlotStatus::Busy(owner)
    );
}

#[test]
fn measured_crash_shape_recovers_all_275_rows_and_allows_the_next_apply() {
    let dir = tempfile::tempdir().unwrap();
    let db = crate::db::Db::open_migrated(Some(&dir.path().join("reprise.db"))).unwrap();
    db.conn()
        .execute(
            "INSERT INTO library_doctor_scans \
             (id, scope_kind, created_at, remote_enabled, checked_tracks, skipped_tracks) \
             VALUES (1, 'selection', 1724964804, 0, 275, 0)",
            [],
        )
        .unwrap();
    db.conn()
        .execute(
            "INSERT INTO tag_write_jobs \
             (id, kind, scan_id, state, created_at, total_tracks) \
             VALUES (19, 'doctor_apply', 1, 'running', 1724964804, 275)",
            [],
        )
        .unwrap();
    for position in 0..275_i64 {
        let (state, written, outcome) = if position < 18 {
            ("complete", 1, "applied")
        } else if position == 18 {
            ("running", 0, "prepared")
        } else {
            ("pending", 0, "pending")
        };
        db.conn()
            .execute(
                "INSERT INTO tag_write_job_files \
                 (job_id, position, track_id, path, state, file_written) \
                 VALUES (19, ?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    position,
                    position + 1,
                    dir.path()
                        .join(format!("track-{position}.flac"))
                        .to_string_lossy(),
                    state,
                    written
                ],
            )
            .unwrap();
        let file_id = db.conn().last_insert_rowid();
        db.conn()
            .execute(
                "INSERT INTO tag_write_journal \
                 (file_id, position, review_row_id, field, guard_is_set, expected_is_null, \
                  before_value, before_is_null, after_value, after_is_null, outcome) \
                 VALUES (?1, 0, ?2, 'title', 0, 1, 'Before', 0, 'After', 0, ?3)",
                rusqlite::params![file_id, position + 1, outcome],
            )
            .unwrap();
    }

    assert!(matches!(
        LibraryDoctor::new(&db)
            .tag_write_slot_status(dir.path())
            .unwrap(),
        TagWriteSlotStatus::Orphaned(TagWriteSlotOwner {
            job_id: 19,
            completed_tracks: 18,
            total_tracks: 275,
            ..
        })
    ));
    assert_eq!(
        LibraryDoctor::new(&db)
            .tag_write_slot_status(dir.path())
            .unwrap(),
        TagWriteSlotStatus::Free
    );
    assert!(super::write::prepare_job(db.conn(), "doctor_apply", None, Some(1), &[],).is_ok());
}
