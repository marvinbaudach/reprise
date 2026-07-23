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

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    #[test]
    fn migration_v19_creates_library_doctor_snapshot_tables() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();

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
        assert_eq!(version, 27);
        assert_eq!(table_count, 8);
    }

    #[test]
    fn migration_v18_to_v19_preserves_tracks_and_enforces_doctor_invariants() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrate(&conn).unwrap();
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
}
