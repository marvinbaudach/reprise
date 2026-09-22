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

pub(crate) fn migrate_v58(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 58 {
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
    transaction.pragma_update(None, "user_version", 58)?;
    transaction.commit()
}

pub(crate) fn migrate_v66(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 66 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    let never_preselect_column_exists = transaction.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM pragma_table_info('library_doctor_proposals')
           WHERE name='never_preselect'
         )",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    if !never_preselect_column_exists {
        transaction.execute(
            "ALTER TABLE library_doctor_proposals
             ADD COLUMN never_preselect INTEGER NOT NULL DEFAULT 0
             CHECK (never_preselect IN (0, 1))",
            [],
        )?;
    }
    transaction.execute(
        "UPDATE library_doctor_state
         SET last_complete_scan_id=NULL, reviewed_scan_id=NULL",
        [],
    )?;
    transaction.pragma_update(None, "user_version", 66)?;
    transaction.commit()
}

pub(crate) fn migrate_v67(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 67 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    let column_exists = transaction.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM pragma_table_info('library_doctor_proposals')
           WHERE name='resolved_release_mbid'
         )",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    if !column_exists {
        transaction.execute(
            "ALTER TABLE library_doctor_proposals
             ADD COLUMN resolved_release_mbid TEXT",
            [],
        )?;
    }
    transaction.execute("DELETE FROM library_doctor_remote_cache", [])?;
    transaction.execute(
        "UPDATE library_doctor_state
         SET last_complete_scan_id=NULL, reviewed_scan_id=NULL",
        [],
    )?;
    transaction.pragma_update(None, "user_version", 67)?;
    transaction.commit()
}

pub(crate) fn migrate_v86(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 86 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute(
        &format!(
            "UPDATE library_doctor_scan_tracks AS s
             SET title = CASE WHEN EXISTS (
                   SELECT 1 FROM tag_write_journal v
                   JOIN tag_write_job_files f ON f.id=v.file_id
                   JOIN tag_write_jobs j ON j.id=f.job_id
                   WHERE f.track_id=s.track_id AND j.kind='doctor_apply'
                     AND v.field='title' AND v.outcome='applied'
                 ) THEN (SELECT t.title FROM tracks t WHERE t.id=s.track_id) ELSE s.title END,
                 artist = CASE WHEN EXISTS (
                   SELECT 1 FROM tag_write_journal v
                   JOIN tag_write_job_files f ON f.id=v.file_id
                   JOIN tag_write_jobs j ON j.id=f.job_id
                   WHERE f.track_id=s.track_id AND j.kind='doctor_apply'
                     AND v.field='artist' AND v.outcome='applied'
                 ) THEN (SELECT t.artist FROM tracks t WHERE t.id=s.track_id) ELSE s.artist END,
                 album = CASE WHEN EXISTS (
                   SELECT 1 FROM tag_write_journal v
                   JOIN tag_write_job_files f ON f.id=v.file_id
                   JOIN tag_write_jobs j ON j.id=f.job_id
                   WHERE f.track_id=s.track_id AND j.kind='doctor_apply'
                     AND v.field='album' AND v.outcome='applied'
                 ) THEN (SELECT t.album FROM tracks t WHERE t.id=s.track_id) ELSE s.album END,
                 album_artist = CASE WHEN EXISTS (
                   SELECT 1 FROM tag_write_journal v
                   JOIN tag_write_job_files f ON f.id=v.file_id
                   JOIN tag_write_jobs j ON j.id=f.job_id
                   WHERE f.track_id=s.track_id AND j.kind='doctor_apply'
                     AND v.field='album_artist' AND v.outcome='applied'
                 ) THEN (SELECT t.album_artist FROM tracks t WHERE t.id=s.track_id)
                   ELSE s.album_artist END,
                 year = CASE WHEN EXISTS (
                   SELECT 1 FROM tag_write_journal v
                   JOIN tag_write_job_files f ON f.id=v.file_id
                   JOIN tag_write_jobs j ON j.id=f.job_id
                   WHERE f.track_id=s.track_id AND j.kind='doctor_apply'
                     AND v.field='year' AND v.outcome='applied'
                 ) THEN (SELECT t.year FROM tracks t WHERE t.id=s.track_id) ELSE s.year END,
                 track_no = CASE WHEN EXISTS (
                   SELECT 1 FROM tag_write_journal v
                   JOIN tag_write_job_files f ON f.id=v.file_id
                   JOIN tag_write_jobs j ON j.id=f.job_id
                   WHERE f.track_id=s.track_id AND j.kind='doctor_apply'
                     AND v.field='track_no' AND v.outcome='applied'
                 ) THEN (SELECT t.track_no FROM tracks t WHERE t.id=s.track_id) ELSE s.track_no END,
                 genre = CASE WHEN EXISTS (
                   SELECT 1 FROM tag_write_journal v
                   JOIN tag_write_job_files f ON f.id=v.file_id
                   JOIN tag_write_jobs j ON j.id=f.job_id
                   WHERE f.track_id=s.track_id AND j.kind='doctor_apply'
                     AND v.field='genre' AND v.outcome='applied'
                 ) THEN (SELECT t.genre FROM tracks t WHERE t.id=s.track_id) ELSE s.genre END
             WHERE s.scan_id=(
                     SELECT last_complete_scan_id FROM library_doctor_state WHERE singleton=1
                   )
               AND s.read_ok=1
               AND EXISTS (SELECT 1 FROM tracks t WHERE t.id=s.track_id AND {})
               AND EXISTS (
                 SELECT 1 FROM tag_write_journal v
                 JOIN tag_write_job_files f ON f.id=v.file_id
                 JOIN tag_write_jobs j ON j.id=f.job_id
                 WHERE f.track_id=s.track_id AND j.kind='doctor_apply'
                   AND v.outcome='applied'
               )",
            crate::queries::PRESENT
        ),
        [],
    )?;
    transaction.pragma_update(None, "user_version", 86)?;
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

        super::migrate_v58(conn).unwrap();

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

    #[test]
    fn migration_v65_to_v66_preserves_existing_scans_and_is_idempotent() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let conn = db.conn();
        conn.execute(
            "INSERT INTO library_doctor_scans
             (scope_kind, created_at, remote_enabled, checked_tracks, skipped_tracks)
             VALUES ('whole_library', 1, 0, 0, 0)",
            [],
        )
        .unwrap();
        conn.execute_batch(
            "ALTER TABLE library_doctor_proposals DROP COLUMN never_preselect;
             PRAGMA user_version=63;",
        )
        .unwrap();

        super::migrate_v66(conn).unwrap();
        super::migrate_v66(conn).unwrap();

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        let scans: i64 = conn
            .query_row("SELECT COUNT(*) FROM library_doctor_scans", [], |row| {
                row.get(0)
            })
            .unwrap();
        let column: (String, i64, String) = conn
            .query_row(
                "SELECT type, \"notnull\", dflt_value
                 FROM pragma_table_info('library_doctor_proposals')
                 WHERE name='never_preselect'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!((version, scans), (66, 1));
        assert_eq!(column, ("INTEGER".into(), 1, "0".into()));
    }

    #[test]
    fn doc_10c_the_guard_rail_upgrade_clears_the_stored_scan_pointer() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let conn = db.conn();
        conn.execute(
            "INSERT INTO library_doctor_scans
             (scope_kind, created_at, remote_enabled, checked_tracks, skipped_tracks)
             VALUES ('selection', 1, 0, 0, 0)",
            [],
        )
        .unwrap();
        let scan_id = conn.last_insert_rowid();
        conn.execute(
            "UPDATE library_doctor_state
             SET last_complete_scan_id=?1, reviewed_scan_id=?1 WHERE singleton=1",
            [scan_id],
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 63).unwrap();

        super::migrate_v66(conn).unwrap();

        let pointers: (Option<i64>, Option<i64>) = conn
            .query_row(
                "SELECT last_complete_scan_id, reviewed_scan_id
                 FROM library_doctor_state WHERE singleton=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(pointers, (None, None));
    }

    #[test]
    fn migration_v66_to_v67_preserves_existing_scans_and_is_idempotent() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let conn = db.conn();
        conn.execute(
            "INSERT INTO library_doctor_scans
             (scope_kind, created_at, remote_enabled, checked_tracks, skipped_tracks)
             VALUES ('whole_library', 1, 1, 0, 0)",
            [],
        )
        .unwrap();
        let scan_id = conn.last_insert_rowid();
        conn.execute(
            "UPDATE library_doctor_state
             SET last_complete_scan_id=?1, reviewed_scan_id=?1 WHERE singleton=1",
            [scan_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO library_doctor_remote_cache
             (cache_key, fetched_at, expires_at, result_json)
             VALUES ('old-shape', 1, 2, '{}')",
            [],
        )
        .unwrap();
        conn.execute_batch(
            "ALTER TABLE library_doctor_proposals DROP COLUMN resolved_release_mbid;
             PRAGMA user_version=66;",
        )
        .unwrap();

        super::migrate_v67(conn).unwrap();
        super::migrate_v67(conn).unwrap();

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        let scans: i64 = conn
            .query_row("SELECT COUNT(*) FROM library_doctor_scans", [], |row| {
                row.get(0)
            })
            .unwrap();
        let column_type: String = conn
            .query_row(
                "SELECT type FROM pragma_table_info('library_doctor_proposals')
                 WHERE name='resolved_release_mbid'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let cache_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM library_doctor_remote_cache",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let pointers: (Option<i64>, Option<i64>) = conn
            .query_row(
                "SELECT last_complete_scan_id, reviewed_scan_id
                 FROM library_doctor_state WHERE singleton=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert_eq!((version, scans), (67, 1));
        assert_eq!(column_type, "TEXT");
        assert_eq!(cache_rows, 0);
        assert_eq!(pointers, (None, None));
    }

    #[test]
    fn migration_v85_to_v86_repairs_written_snapshot_rows_and_is_idempotent() {
        let db = crate::db::Db::open_in_memory().unwrap();
        let conn = db.conn();
        conn.execute_batch(
            "INSERT INTO tracks
               (id, path, title, artist, album, album_artist, year, track_no, genre,
                added_at, file_mtime, file_size)
             VALUES
               (1, 'written.flac', 'Current title', 'Reformist', 'Current album',
                'Current album artist', 2026, 1, 'Current genre', 0, 10, 100),
               (2, 'untouched.flac', 'Other current title', 'Other current artist',
                'Other current album', 'Other current album artist', 2025, 2,
                'Other current genre', 0, 20, 200);
             INSERT INTO library_doctor_scans
               (id, scope_kind, created_at, remote_enabled, checked_tracks, skipped_tracks)
             VALUES
               (10, 'whole_library', 1, 1, 2, 0),
               (20, 'whole_library', 2, 1, 2, 0);
             INSERT INTO library_doctor_scan_tracks
               (scan_id, position, track_id, path, file_mtime, file_size, device, inode,
                read_ok, title, artist, album, album_artist, year, track_no, genre)
             VALUES
               (20, 0, 1, 'written.flac', 10, 100, NULL, NULL, 1,
                'Snapshot title', 'REFORMIST', 'Snapshot album',
                'Snapshot album artist', 2000, 7, 'Snapshot genre'),
               (20, 1, 2, 'untouched.flac', 20, 200, NULL, NULL, 1,
                'Untouched title', 'Untouched artist', 'Untouched album',
                'Untouched album artist', 1999, 8, 'Untouched genre');
             UPDATE library_doctor_state
             SET last_complete_scan_id=20 WHERE singleton=1;
             INSERT INTO tag_write_jobs
               (id, kind, source_job_id, scan_id, state, created_at, finished_at, total_tracks)
             VALUES (30, 'doctor_apply', NULL, 10, 'completed', 1, 2, 1);
             INSERT INTO tag_write_job_files
               (id, job_id, position, track_id, path, state, file_written)
             VALUES (40, 30, 0, 1, 'written.flac', 'complete', 1);
             INSERT INTO tag_write_journal
               (file_id, position, review_row_id, field, guard_is_set, expected_value,
                expected_is_null, before_value, before_is_null, after_value,
                after_is_null, outcome)
             VALUES
               (40, 0, NULL, 'artist', 1, 'REFORMIST', 0, 'REFORMIST', 0,
                'Reformist', 0, 'applied');
             PRAGMA user_version=85;",
        )
        .unwrap();

        super::migrate_v86(conn).unwrap();
        let repaired = conn
            .query_row(
                "SELECT title, artist, album, album_artist, year, track_no, genre
                 FROM library_doctor_scan_tracks WHERE scan_id=20 AND track_id=1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<u32>>(4)?,
                        row.get::<_, Option<u32>>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                },
            )
            .unwrap();
        let untouched = conn
            .query_row(
                "SELECT title, artist, album, album_artist, year, track_no, genre
                 FROM library_doctor_scan_tracks WHERE scan_id=20 AND track_id=2",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<u32>>(4)?,
                        row.get::<_, Option<u32>>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            repaired,
            (
                "Snapshot title".into(),
                "Reformist".into(),
                "Snapshot album".into(),
                "Snapshot album artist".into(),
                Some(2000),
                Some(7),
                "Snapshot genre".into(),
            )
        );
        assert_eq!(
            untouched,
            (
                "Untouched title".into(),
                "Untouched artist".into(),
                "Untouched album".into(),
                "Untouched album artist".into(),
                Some(1999),
                Some(8),
                "Untouched genre".into(),
            )
        );

        conn.execute("UPDATE tracks SET artist='Later' WHERE id=1", [])
            .unwrap();
        super::migrate_v86(conn).unwrap();
        let after_second_run: String = conn
            .query_row(
                "SELECT artist FROM library_doctor_scan_tracks
                 WHERE scan_id=20 AND track_id=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(after_second_run, "Reformist");
        assert_eq!(version, 86);
    }
}
