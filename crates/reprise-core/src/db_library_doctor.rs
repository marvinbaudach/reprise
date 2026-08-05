use rusqlite::Connection;

const SCHEMA_V18: &str = r#"
CREATE TABLE library_doctor_scans (
  id              INTEGER PRIMARY KEY,
  scope_kind      TEXT NOT NULL,
  created_at      INTEGER NOT NULL CHECK (created_at >= 0),
  remote_enabled  INTEGER NOT NULL CHECK (remote_enabled IN (0, 1)),
  checked_tracks  INTEGER NOT NULL CHECK (checked_tracks >= 0),
  skipped_tracks  INTEGER NOT NULL CHECK (skipped_tracks >= 0),
  CHECK (scope_kind IN ('whole_library', 'current_view', 'selection'))
);
CREATE TABLE library_doctor_state (
  singleton              INTEGER PRIMARY KEY CHECK (singleton = 1),
  last_complete_scan_id  INTEGER REFERENCES library_doctor_scans(id) ON DELETE SET NULL
);
INSERT INTO library_doctor_state (singleton, last_complete_scan_id) VALUES (1, NULL);
CREATE TABLE library_doctor_scan_tracks (
  scan_id      INTEGER NOT NULL REFERENCES library_doctor_scans(id) ON DELETE CASCADE,
  position     INTEGER NOT NULL,
  track_id     INTEGER NOT NULL,
  path         TEXT NOT NULL,
  file_mtime   INTEGER NOT NULL,
  file_size    INTEGER NOT NULL,
  device       INTEGER,
  inode        INTEGER,
  read_ok      INTEGER NOT NULL CHECK (read_ok IN (0, 1)),
  title        TEXT,
  artist       TEXT,
  album        TEXT,
  album_artist TEXT,
  year         INTEGER,
  track_no     INTEGER,
  genre        TEXT,
  PRIMARY KEY (scan_id, position),
  UNIQUE (scan_id, track_id)
);
CREATE TABLE library_doctor_proposals (
  id              INTEGER PRIMARY KEY,
  scan_id         INTEGER NOT NULL REFERENCES library_doctor_scans(id) ON DELETE CASCADE,
  position        INTEGER NOT NULL,
  track_id        INTEGER NOT NULL,
  field           TEXT NOT NULL,
  current_value   TEXT,
  proposed_value  TEXT,
  source          TEXT NOT NULL,
  confidence      INTEGER NOT NULL CHECK (confidence BETWEEN 0 AND 100),
  preselected     INTEGER NOT NULL CHECK (preselected IN (0, 1)),
  problem_class   TEXT NOT NULL,
  CHECK (field IN ('title', 'artist', 'album', 'album_artist', 'year', 'genre', 'recording_mbid')),
  CHECK (source IN ('local', 'musicbrainz', 'acoustid')),
  CHECK (problem_class IN ('casing_whitespace', 'missing_album_artist', 'genre_variant', 'missing_wrong_year', 'missing_recording_mbid')),
  UNIQUE (scan_id, track_id, field)
);
CREATE INDEX idx_library_doctor_proposals_scan
ON library_doctor_proposals(scan_id, position);
CREATE TABLE library_doctor_groups (
  id          INTEGER PRIMARY KEY,
  scan_id     INTEGER NOT NULL REFERENCES library_doctor_scans(id) ON DELETE CASCADE,
  position    INTEGER NOT NULL,
  field       TEXT NOT NULL,
  group_key   TEXT NOT NULL,
  CHECK (field IN ('title', 'artist', 'album', 'album_artist', 'year', 'genre', 'recording_mbid')),
  UNIQUE (scan_id, field, group_key)
);
CREATE TABLE library_doctor_group_candidates (
  group_id         INTEGER NOT NULL REFERENCES library_doctor_groups(id) ON DELETE CASCADE,
  position         INTEGER NOT NULL,
  candidate_value  TEXT NOT NULL,
  candidate_count  INTEGER NOT NULL,
  PRIMARY KEY (group_id, position),
  UNIQUE (group_id, candidate_value),
  CHECK (candidate_count > 0)
);
CREATE TABLE library_doctor_group_members (
  group_id   INTEGER NOT NULL REFERENCES library_doctor_groups(id) ON DELETE CASCADE,
  position   INTEGER NOT NULL,
  track_id      INTEGER NOT NULL,
  current_value TEXT,
  PRIMARY KEY (group_id, position),
  UNIQUE (group_id, track_id)
);
"#;

pub(crate) fn migrate_v19(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 19 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(SCHEMA_V18)?;
    transaction.pragma_update(None, "user_version", 19)?;
    transaction.commit()
}

pub(crate) fn migrate_v57(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 57 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    let reviewed_column_exists = transaction.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM pragma_table_info('library_doctor_state')
           WHERE name='reviewed_scan_id'
         )",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    if !reviewed_column_exists {
        transaction.execute(
            "ALTER TABLE library_doctor_state ADD COLUMN reviewed_scan_id INTEGER \
             REFERENCES library_doctor_scans(id) ON DELETE SET NULL",
            [],
        )?;
    }
    transaction.execute(
        "UPDATE library_doctor_state \
         SET last_complete_scan_id=NULL, reviewed_scan_id=NULL",
        [],
    )?;
    transaction.pragma_update(None, "user_version", 57)?;
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    #[test]
    fn migration_v19_creates_library_doctor_snapshot_tables() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate_connection(&conn).unwrap();

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        let table_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name LIKE 'library_doctor_%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, crate::db::SUPPORTED_SCHEMA_VERSION);
        assert_eq!(table_count, 8);
    }

    #[test]
    fn migration_v18_to_v19_preserves_tracks_and_enforces_doctor_invariants() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrate_connection(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks (path, title, added_at) VALUES ('keep.flac', 'Keep', 1)",
            [],
        )
        .unwrap();
        conn.execute_batch(
            "DROP TABLE library_doctor_remote_cache;
             DROP TABLE library_doctor_state;
             DROP TABLE library_doctor_group_members;
             DROP TABLE library_doctor_group_candidates;
             DROP TABLE library_doctor_groups;
             DROP TABLE library_doctor_proposals;
             DROP TABLE library_doctor_scan_tracks;
             DROP TABLE library_doctor_scans;
             PRAGMA user_version=18;",
        )
        .unwrap();

        super::migrate_v19(&conn).unwrap();
        super::migrate_v19(&conn).unwrap();

        let title: String = conn
            .query_row(
                "SELECT title FROM tracks WHERE path='keep.flac'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(title, "Keep");
        assert!(conn
            .execute(
                "INSERT INTO library_doctor_scans \
                 (scope_kind, created_at, remote_enabled, checked_tracks, skipped_tracks) \
                 VALUES ('invalid', 0, 0, 0, 0)",
                [],
            )
            .is_err());
    }

    #[test]
    fn doc_10c_upgrade_clears_the_stored_scan_pointer_and_keeps_the_cleanup_revertible() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let conn = db.conn();
        conn.execute_batch(
            "DROP TABLE library_doctor_state;
             CREATE TABLE library_doctor_state (
               singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
               last_complete_scan_id INTEGER REFERENCES library_doctor_scans(id) ON DELETE SET NULL
             );
             INSERT INTO library_doctor_state (singleton, last_complete_scan_id)
             VALUES (1, NULL);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO library_doctor_scans \
             (scope_kind, created_at, remote_enabled, checked_tracks, skipped_tracks) \
             VALUES ('selection', 1, 0, 1, 0)",
            [],
        )
        .unwrap();
        let scan_id = conn.last_insert_rowid();
        conn.execute(
            "UPDATE library_doctor_state SET last_complete_scan_id=?1 WHERE singleton=1",
            [scan_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tag_write_jobs \
             (kind, source_job_id, scan_id, state, created_at, finished_at, total_tracks) \
             VALUES ('doctor_apply', NULL, ?1, 'completed', 1, 2, 1)",
            [scan_id],
        )
        .unwrap();
        let job_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO tag_write_job_files \
             (job_id, position, track_id, path, state, file_written) \
             VALUES (?1, 0, 42, 'fixture.flac', 'complete', 1)",
            [job_id],
        )
        .unwrap();
        let file_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO tag_write_journal \
             (file_id, position, review_row_id, field, guard_is_set, expected_value, \
              expected_is_null, before_value, before_is_null, after_value, after_is_null, outcome) \
             VALUES (?1, 0, 1, 'artist', 1, 'Before', 0, 'Before', 0, 'After', 0, 'applied')",
            [file_id],
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 56).unwrap();

        super::migrate_v57(conn).unwrap();

        let pointers: (Option<i64>, Option<i64>) = conn
            .query_row(
                "SELECT last_complete_scan_id, reviewed_scan_id \
                 FROM library_doctor_state WHERE singleton=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(pointers, (None, None));
        assert!(crate::library::library_doctor::LibraryDoctor::new(&db)
            .last_cleanup()
            .unwrap()
            .is_some());
    }
}
