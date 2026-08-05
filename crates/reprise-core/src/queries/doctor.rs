use std::collections::HashSet;

use crate::db::Db;
use crate::library_doctor::{DoctorField, ProposalSource};

pub fn count_pending_doctor_findings(db: &Db) -> Result<u32, rusqlite::Error> {
    let scan_id = db.conn().query_row(
        "SELECT CASE WHEN last_complete_scan_id IS NULL
                       OR last_complete_scan_id = reviewed_scan_id
                     THEN NULL ELSE last_complete_scan_id END
         FROM library_doctor_state WHERE singleton=1",
        [],
        |row| row.get::<_, Option<i64>>(0),
    )?;
    let Some(scan_id) = scan_id else {
        return Ok(0);
    };

    let mut applied_statement = db.conn().prepare(
        "SELECT f.track_id, v.field FROM tag_write_journal v
         JOIN tag_write_job_files f ON v.file_id = f.id
         JOIN tag_write_jobs j ON j.id = f.job_id
         WHERE j.scan_id = ?1 AND j.kind = 'doctor_apply' AND v.outcome = 'applied'",
    )?;
    let applied = applied_statement
        .query_map([scan_id], |row| {
            let raw_field = row.get::<_, String>(1)?;
            let field = DoctorField::parse(&raw_field).ok_or_else(|| {
                rusqlite::Error::InvalidColumnType(1, raw_field, rusqlite::types::Type::Text)
            })?;
            Ok((row.get::<_, i64>(0)?, field))
        })?
        .collect::<Result<HashSet<_>, _>>()?;

    let mut proposal_statement = db.conn().prepare(
        "SELECT track_id, field, source, preselected
         FROM library_doctor_proposals WHERE scan_id=?1",
    )?;
    let proposals = proposal_statement.query_map([scan_id], |row| {
        let raw_field = row.get::<_, String>(1)?;
        let field = DoctorField::parse(&raw_field).ok_or_else(|| {
            rusqlite::Error::InvalidColumnType(1, raw_field, rusqlite::types::Type::Text)
        })?;
        let raw_source = row.get::<_, String>(2)?;
        let source = ProposalSource::parse(&raw_source).ok_or_else(|| {
            rusqlite::Error::InvalidColumnType(2, raw_source, rusqlite::types::Type::Text)
        })?;
        Ok((row.get::<_, i64>(0)?, field, source, row.get::<_, bool>(3)?))
    })?;
    let mut pending = 0u32;
    for proposal in proposals {
        let (track_id, field, source, preselected) = proposal?;
        if !crate::library_doctor::is_auto_applied_parts(field, source, preselected, false)
            && !applied.contains(&(track_id, field))
        {
            pending = pending.saturating_add(1);
        }
    }
    Ok(pending)
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

    fn seed_proposal(db: &crate::db::Db, scan_id: i64, position: i64, track_id: i64, source: &str) {
        db.conn()
            .execute(
                "INSERT INTO library_doctor_proposals \
                 (scan_id, position, track_id, field, current_value, proposed_value, source, \
                  confidence, preselected, problem_class, evidence_json, local_fallback_json) \
                 VALUES (?1, ?2, ?3, 'artist', ' Before ', 'Before', ?4, \
                         100, 1, 'casing_whitespace', '[]', 'null')",
                rusqlite::params![scan_id, position, track_id, source],
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

    fn seed_conflicting_change(db: &crate::db::Db, scan_id: i64, track_id: i64) {
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
                 (job_id, position, track_id, path, state, file_written, error_kind, \
                  error_message) \
                 VALUES (?1, 0, ?2, 'fixture.flac', 'failed', 0, 'io', \
                         'all selected fields conflict')",
                rusqlite::params![job_id, track_id],
            )
            .unwrap();
        let file_id = db.conn().last_insert_rowid();
        db.conn()
            .execute(
                "INSERT INTO tag_write_journal \
                 (file_id, position, review_row_id, field, guard_is_set, expected_value, \
                  expected_is_null, before_value, before_is_null, after_value, after_is_null, outcome) \
                 VALUES (?1, 0, 1, 'artist', 1, ' Before ', 0, ' Before ', 0, 'Before', 0, 'conflict')",
                [file_id],
            )
            .unwrap();
    }

    #[test]
    fn doc_8a_pending_review_count_excludes_everything_already_written_for_that_scan() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let scan_id = seed_scan(&db);
        seed_proposal(&db, scan_id, 0, 1, "musicbrainz");
        seed_proposal(&db, scan_id, 1, 2, "musicbrainz");
        seed_applied_change(&db, scan_id, 1);

        assert_eq!(super::count_pending_doctor_findings(&db).unwrap(), 1);
    }

    #[test]
    fn doc_8a_pending_review_count_is_zero_once_the_scan_is_marked_reviewed() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let scan_id = seed_scan(&db);
        seed_proposal(&db, scan_id, 0, 1, "musicbrainz");
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

    #[test]
    fn doc_8a_auto_tier_write_conflict_does_not_produce_a_pending_count() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let scan_id = seed_scan(&db);
        seed_proposal(&db, scan_id, 0, 1, "local");
        seed_conflicting_change(&db, scan_id, 1);

        assert_eq!(super::count_pending_doctor_findings(&db).unwrap(), 0);
    }
}
