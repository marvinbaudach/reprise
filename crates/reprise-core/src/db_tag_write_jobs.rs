use rusqlite::Connection;

const SCHEMA_V19: &str = r#"
CREATE TABLE tag_write_jobs (
  id             INTEGER PRIMARY KEY,
  kind           TEXT NOT NULL CHECK (kind IN ('tag_editor', 'doctor_apply', 'doctor_revert')),
  source_job_id  INTEGER REFERENCES tag_write_jobs(id) ON DELETE RESTRICT,
  scan_id        INTEGER REFERENCES library_doctor_scans(id) ON DELETE RESTRICT,
  state          TEXT NOT NULL CHECK (state IN ('prepared', 'running', 'completed', 'cancelled', 'interrupted')),
  created_at     INTEGER NOT NULL CHECK (created_at >= 0),
  finished_at    INTEGER CHECK (finished_at >= created_at),
  total_tracks   INTEGER NOT NULL CHECK (total_tracks >= 0),
  CHECK ((state IN ('prepared', 'running')) = (finished_at IS NULL)),
  CHECK (
    (kind = 'tag_editor' AND source_job_id IS NULL AND scan_id IS NULL) OR
    (kind = 'doctor_apply' AND source_job_id IS NULL AND scan_id IS NOT NULL) OR
    (kind = 'doctor_revert' AND source_job_id IS NOT NULL)
  ),
  CHECK (source_job_id IS NULL OR source_job_id <> id)
);
CREATE TABLE tag_write_job_files (
  id             INTEGER PRIMARY KEY,
  job_id         INTEGER NOT NULL REFERENCES tag_write_jobs(id) ON DELETE CASCADE,
  position       INTEGER NOT NULL CHECK (position >= 0),
  track_id       INTEGER NOT NULL,
  path           TEXT NOT NULL CHECK (path <> ''),
  state          TEXT NOT NULL CHECK (state IN ('pending', 'running', 'complete', 'cancelled', 'unavailable', 'failed')),
  error_kind     TEXT CHECK (error_kind IN ('permission_denied', 'not_found', 'unsupported_format', 'unreadable_tags', 'io')),
  error_message  TEXT,
  file_written   INTEGER NOT NULL DEFAULT 0 CHECK (file_written IN (0, 1)),
  UNIQUE (job_id, position),
  UNIQUE (job_id, track_id),
  CHECK ((state IN ('failed', 'unavailable')) = (error_kind IS NOT NULL)),
  CHECK ((error_kind IS NULL) = (error_message IS NULL)),
  CHECK (state NOT IN ('pending', 'running', 'cancelled', 'unavailable') OR file_written = 0),
  CHECK (state <> 'complete' OR file_written = 1)
);
CREATE TABLE tag_write_journal (
  file_id          INTEGER NOT NULL REFERENCES tag_write_job_files(id) ON DELETE CASCADE,
  position         INTEGER NOT NULL CHECK (position >= 0),
  review_row_id    INTEGER,
  field            TEXT NOT NULL CHECK (field IN ('title', 'artist', 'album', 'album_artist', 'year', 'track_no', 'genre', 'recording_mbid')),
  guard_is_set     INTEGER NOT NULL CHECK (guard_is_set IN (0, 1)),
  expected_value   TEXT,
  expected_is_null INTEGER NOT NULL CHECK (expected_is_null IN (0, 1)),
  before_value     TEXT,
  before_is_null   INTEGER NOT NULL CHECK (before_is_null IN (0, 1)),
  after_value      TEXT,
  after_is_null    INTEGER NOT NULL CHECK (after_is_null IN (0, 1)),
  outcome          TEXT NOT NULL CHECK (outcome IN ('pending', 'prepared', 'applied', 'not_applied', 'conflict', 'unavailable', 'failed', 'reverted')),
  PRIMARY KEY (file_id, field),
  UNIQUE (file_id, position),
  UNIQUE (file_id, review_row_id),
  CHECK (review_row_id IS NULL OR review_row_id >= 0),
  CHECK ((expected_is_null = 1) = (expected_value IS NULL)),
  CHECK ((guard_is_set = 1) OR (expected_is_null = 1)),
  CHECK ((before_is_null = 1) = (before_value IS NULL)),
  CHECK ((after_is_null = 1) = (after_value IS NULL)),
  CHECK (field IN ('year', 'track_no') OR (before_is_null = 0 AND after_is_null = 0)),
  CHECK (field IN ('year', 'track_no') OR guard_is_set = 0 OR expected_is_null = 0)
);
CREATE INDEX idx_tag_write_job_files_job ON tag_write_job_files(job_id, position);
CREATE TRIGGER tag_write_jobs_identity_immutable
BEFORE UPDATE ON tag_write_jobs
WHEN NEW.kind IS NOT OLD.kind
  OR NEW.source_job_id IS NOT OLD.source_job_id
  OR NEW.scan_id IS NOT OLD.scan_id
  OR NEW.created_at IS NOT OLD.created_at
  OR NEW.total_tracks IS NOT OLD.total_tracks
BEGIN
  SELECT RAISE(ABORT, 'tag-write job identity and configuration are immutable');
END;
CREATE TRIGGER tag_write_job_files_identity_immutable
BEFORE UPDATE ON tag_write_job_files
WHEN NEW.job_id IS NOT OLD.job_id
  OR NEW.position IS NOT OLD.position
  OR NEW.track_id IS NOT OLD.track_id
  OR NEW.path IS NOT OLD.path
BEGIN
  SELECT RAISE(ABORT, 'tag-write file identity and target are immutable');
END;
CREATE TRIGGER tag_write_journal_identity_immutable
BEFORE UPDATE ON tag_write_journal
WHEN NEW.file_id IS NOT OLD.file_id
  OR NEW.position IS NOT OLD.position
  OR NEW.review_row_id IS NOT OLD.review_row_id
  OR NEW.field IS NOT OLD.field
  OR NEW.guard_is_set IS NOT OLD.guard_is_set
  OR NEW.expected_value IS NOT OLD.expected_value
  OR NEW.expected_is_null IS NOT OLD.expected_is_null
  OR NEW.before_value IS NOT OLD.before_value
  OR NEW.before_is_null IS NOT OLD.before_is_null
  OR NEW.after_value IS NOT OLD.after_value
  OR NEW.after_is_null IS NOT OLD.after_is_null
BEGIN
  SELECT RAISE(ABORT, 'tag-write journal identity and values are immutable');
END;
"#;

pub(crate) fn migrate_v20(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 20 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(SCHEMA_V19)?;
    transaction.pragma_update(None, "user_version", 20)?;
    transaction.commit()
}

#[cfg(test)]
mod tests {
    #[test]
    fn migration_v19_has_future_complete_job_and_field_states() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate_connection(&conn).unwrap();

        conn.execute(
            "INSERT INTO tag_write_jobs \
             (kind, source_job_id, scan_id, state, created_at, finished_at, total_tracks) \
             VALUES ('tag_editor', NULL, NULL, 'prepared', 0, NULL, 1)",
            [],
        )
        .unwrap();
        let job_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO tag_write_job_files \
             (job_id, position, track_id, path, state, file_written) \
             VALUES (?1, 0, 42, 'fixture.flac', 'pending', 0)",
            [job_id],
        )
        .unwrap();
        let file_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO tag_write_journal \
             (file_id, position, review_row_id, field, guard_is_set, expected_value, \
              expected_is_null, before_value, before_is_null, after_value, after_is_null, outcome) \
             VALUES (?1, 0, NULL, 'recording_mbid', 0, NULL, 1, '', 0, 'mbid', 0, 'pending')",
            [file_id],
        )
        .unwrap();
    }

    #[test]
    fn migration_v20_creates_constrained_tag_write_journal() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate_connection(&conn).unwrap();
        conn.execute_batch(
            "DROP TRIGGER tag_write_journal_identity_immutable;
             DROP TABLE tag_write_journal;
             DROP TABLE tag_write_job_files;
             DROP TABLE tag_write_jobs;
             PRAGMA user_version=19;",
        )
        .unwrap();
        super::migrate_v20(&conn).unwrap();

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        let tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type='table' AND name IN \
                 ('tag_write_jobs', 'tag_write_job_files', 'tag_write_journal')",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(version, 20);
        assert_eq!(tables, 3);
        assert!(conn
            .execute(
                "INSERT INTO tag_write_jobs \
                 (kind, state, created_at, total_tracks) \
                 VALUES ('unknown', 'prepared', 0, 0)",
                [],
            )
            .is_err());

        conn.execute(
            "INSERT INTO tag_write_jobs \
             (kind, state, created_at, total_tracks) \
             VALUES ('tag_editor', 'prepared', 0, 1)",
            [],
        )
        .unwrap();
        let job_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO tag_write_job_files \
             (job_id, position, track_id, path, state, file_written) \
             VALUES (?1, 0, 1, 'fixture.flac', 'pending', 0)",
            [job_id],
        )
        .unwrap();
        let file_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO tag_write_journal \
             (file_id, position, field, guard_is_set, expected_is_null, before_value, \
              before_is_null, after_value, after_is_null, outcome) \
             VALUES (?1, 0, 'title', 0, 1, 'Before', 0, 'After', 0, 'pending')",
            [file_id],
        )
        .unwrap();
        assert!(conn
            .execute(
                "UPDATE tag_write_journal SET after_value='Changed' WHERE file_id=?1",
                [file_id],
            )
            .is_err());
        conn.execute(
            "UPDATE tag_write_journal SET outcome='prepared' WHERE file_id=?1",
            [file_id],
        )
        .unwrap();
    }

    #[test]
    fn migration_v18_to_v19_preserves_real_rows_and_is_idempotent() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate_connection(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks (path, title, added_at) VALUES ('keep.flac', 'Keep', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO library_doctor_scans \
             (scope_kind, created_at, remote_enabled, checked_tracks, skipped_tracks) \
             VALUES ('selection', 1, 0, 1, 0)",
            [],
        )
        .unwrap();
        conn.execute_batch(
            "DROP TRIGGER tag_write_journal_identity_immutable;
             DROP TABLE tag_write_journal;
             DROP TABLE tag_write_job_files;
             DROP TABLE tag_write_jobs;
             PRAGMA user_version=19;",
        )
        .unwrap();

        super::migrate_v20(&conn).unwrap();
        super::migrate_v20(&conn).unwrap();

        let preserved: (String, i64) = conn
            .query_row(
                "SELECT title, (SELECT COUNT(*) FROM library_doctor_scans) \
                 FROM tracks WHERE path='keep.flac'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(preserved, ("Keep".into(), 1));
        let foreign_key_errors: i64 = conn
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(foreign_key_errors, 0);

        let scan_id: i64 = conn
            .query_row("SELECT id FROM library_doctor_scans", [], |row| row.get(0))
            .unwrap();
        assert!(conn
            .execute(
                "INSERT INTO tag_write_jobs \
                 (kind, scan_id, state, created_at, total_tracks) \
                 VALUES ('tag_editor', ?1, 'prepared', 0, 0)",
                [scan_id],
            )
            .is_err());
        assert!(conn
            .execute(
                "INSERT INTO tag_write_jobs \
                 (kind, state, created_at, total_tracks) \
                 VALUES ('doctor_apply', 'prepared', 0, 0)",
                [],
            )
            .is_err());
    }

    #[test]
    fn v19_identity_columns_are_immutable_but_runtime_state_can_advance() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate_connection(&conn).unwrap();
        conn.execute(
            "INSERT INTO library_doctor_scans \
             (scope_kind, created_at, remote_enabled, checked_tracks, skipped_tracks) \
             VALUES ('selection', 0, 0, 0, 0)",
            [],
        )
        .unwrap();
        let scan_id = conn.last_insert_rowid();
        for _ in 0..2 {
            conn.execute(
                "INSERT INTO tag_write_jobs \
                 (kind, state, created_at, total_tracks) \
                 VALUES ('tag_editor', 'prepared', 0, 1)",
                [],
            )
            .unwrap();
        }
        let second_job_id = conn.last_insert_rowid();
        let first_job_id = second_job_id - 1;
        conn.execute(
            "INSERT INTO tag_write_job_files \
             (job_id, position, track_id, path, state, file_written) \
             VALUES (?1, 0, 1, 'one.flac', 'pending', 0)",
            [first_job_id],
        )
        .unwrap();
        let file_id = conn.last_insert_rowid();

        for (sql, params) in [
            (
                "UPDATE tag_write_jobs SET kind='doctor_apply' WHERE id=?1",
                vec![first_job_id],
            ),
            (
                "UPDATE tag_write_jobs SET source_job_id=?2 WHERE id=?1",
                vec![first_job_id, second_job_id],
            ),
            (
                "UPDATE tag_write_jobs SET scan_id=?2 WHERE id=?1",
                vec![first_job_id, scan_id],
            ),
            (
                "UPDATE tag_write_jobs SET created_at=2 WHERE id=?1",
                vec![first_job_id],
            ),
            (
                "UPDATE tag_write_jobs SET total_tracks=2 WHERE id=?1",
                vec![first_job_id],
            ),
        ] {
            let error = conn
                .execute(sql, rusqlite::params_from_iter(params))
                .unwrap_err();
            assert!(error.to_string().contains("immutable"), "{error}");
        }
        for sql in [
            "UPDATE tag_write_job_files SET job_id=2 WHERE id=1",
            "UPDATE tag_write_job_files SET position=2 WHERE id=1",
            "UPDATE tag_write_job_files SET track_id=2 WHERE id=1",
            "UPDATE tag_write_job_files SET path='two.flac' WHERE id=1",
        ] {
            let error = conn.execute(sql, []).unwrap_err();
            assert!(error.to_string().contains("immutable"), "{error}");
        }

        conn.execute(
            "UPDATE tag_write_jobs SET state='running' WHERE id=?1",
            [first_job_id],
        )
        .unwrap();
        conn.execute(
            "UPDATE tag_write_jobs SET state='completed', finished_at=3 WHERE id=?1",
            [first_job_id],
        )
        .unwrap();
        conn.execute(
            "UPDATE tag_write_job_files SET state='running' WHERE id=?1",
            [file_id],
        )
        .unwrap();
        conn.execute(
            "UPDATE tag_write_job_files SET state='complete', file_written=1 WHERE id=?1",
            [file_id],
        )
        .unwrap();
    }
}
