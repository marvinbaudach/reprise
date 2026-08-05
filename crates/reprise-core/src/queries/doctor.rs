use crate::db::Db;

pub fn count_pending_doctor_findings(db: &Db) -> Result<u32, rusqlite::Error> {
    let count = db.conn().query_row(
        "SELECT CASE
           WHEN last_complete_scan_id IS NULL
             OR last_complete_scan_id = reviewed_scan_id THEN 0
           ELSE MAX(0,
             (SELECT COUNT(*) FROM library_doctor_proposals
              WHERE scan_id = last_complete_scan_id)
             -
             (SELECT COUNT(*) FROM tag_write_journal v
              JOIN tag_write_job_files f ON v.file_id = f.id
              JOIN tag_write_jobs j ON j.id = f.job_id
              WHERE j.scan_id = last_complete_scan_id
                AND j.kind = 'doctor_apply'
                AND v.outcome = 'applied'))
         END
         FROM library_doctor_state WHERE singleton=1",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(u32::try_from(count).unwrap_or(u32::MAX))
}

#[cfg(test)]
mod tests {
    fn seed_scan(db: &crate::db::Db) -> i64 {
        db.conn()
            .execute(
                "INSERT INTO library_doctor_scans \
                 (scope_kind, created_at, remote_enabled, checked_tracks, skipped_tracks) \
                 VALUES ('selection', 1, 0, 2, 0)",
                [],
            )
            .unwrap();
        let scan_id = db.conn().last_insert_rowid();
        db.conn()
            .execute(
                "UPDATE library_doctor_state SET last_complete_scan_id=?1 WHERE singleton=1",
                [scan_id],
            )
            .unwrap();
        scan_id
    }

    fn seed_proposal(db: &crate::db::Db, scan_id: i64, position: i64, track_id: i64) {
        db.conn()
            .execute(
                "INSERT INTO library_doctor_proposals \
                 (scan_id, position, track_id, field, current_value, proposed_value, source, \
                  confidence, preselected, problem_class, evidence_json, local_fallback_json) \
                 VALUES (?1, ?2, ?3, 'artist', ' Before ', 'Before', 'local', \
                         100, 1, 'casing_whitespace', '[]', 'null')",
                rusqlite::params![scan_id, position, track_id],
            )
            .unwrap();
    }

    fn seed_applied_change(db: &crate::db::Db, scan_id: i64, track_id: i64) {
        db.conn()
            .execute(
                "INSERT INTO tag_write_jobs \
                 (kind, source_job_id, scan_id, state, created_at, finished_at, total_tracks) \
                 VALUES ('doctor_apply', NULL, ?1, 'completed', 1, 2, 1)",
                [scan_id],
            )
            .unwrap();
        let job_id = db.conn().last_insert_rowid();
        db.conn()
            .execute(
                "INSERT INTO tag_write_job_files \
                 (job_id, position, track_id, path, state, file_written) \
                 VALUES (?1, 0, ?2, 'fixture.flac', 'complete', 1)",
                rusqlite::params![job_id, track_id],
            )
            .unwrap();
        let file_id = db.conn().last_insert_rowid();
        db.conn()
            .execute(
                "INSERT INTO tag_write_journal \
                 (file_id, position, review_row_id, field, guard_is_set, expected_value, \
                  expected_is_null, before_value, before_is_null, after_value, after_is_null, outcome) \
                 VALUES (?1, 0, 1, 'artist', 1, ' Before ', 0, ' Before ', 0, 'Before', 0, 'applied')",
                [file_id],
            )
            .unwrap();
    }

    #[test]
    fn doc_8a_pending_review_count_excludes_everything_already_written_for_that_scan() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let scan_id = seed_scan(&db);
        seed_proposal(&db, scan_id, 0, 1);
        seed_proposal(&db, scan_id, 1, 2);
        seed_applied_change(&db, scan_id, 1);

        assert_eq!(super::count_pending_doctor_findings(&db).unwrap(), 1);
    }

    #[test]
    fn doc_8a_pending_review_count_is_zero_once_the_scan_is_marked_reviewed() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let scan_id = seed_scan(&db);
        seed_proposal(&db, scan_id, 0, 1);
        crate::library::library_doctor::set_reviewed_scan(db.conn(), scan_id).unwrap();

        assert_eq!(super::count_pending_doctor_findings(&db).unwrap(), 0);
    }

    #[test]
    fn doc_8a_conflicts_alone_do_not_produce_a_pending_count() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let scan_id = seed_scan(&db);
        db.conn()
            .execute(
                "INSERT INTO library_doctor_groups \
                 (scan_id, position, field, group_key, local_fallback_json) \
                 VALUES (?1, 0, 'artist', 'artist:conflict', 'null')",
                [scan_id],
            )
            .unwrap();

        assert_eq!(super::count_pending_doctor_findings(&db).unwrap(), 0);
    }
}
