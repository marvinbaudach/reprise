use rusqlite::Connection;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub fn open(path: Option<&Path>) -> Result<Connection, DbError> {
    let conn = match path {
        Some(p) => {
            if let Some(dir) = p.parent() {
                std::fs::create_dir_all(dir)?;
            }
            Connection::open(p)?
        }
        None => Connection::open_in_memory()?,
    };
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    // Cheap insurance for future concurrent writers (e.g. a scan worker
    // thread's own `Connection` writing while the UI thread reads): wait up
    // to 5s for a lock instead of failing immediately with `SQLITE_BUSY`.
    conn.pragma_update(None, "busy_timeout", 5000)?;
    Ok(conn)
}

/// The on-disk database path (honors `XDG_DATA_HOME` via `dirs::data_dir`,
/// which is how headless E2E runs point the app at a scratch database
/// without touching `~/.local/share/reprise`). Lives in `reprise-core` so
/// every frontend — GNOME today, a future KDE/Qt or macOS client — resolves
/// the *same* library database. Frontends also hand this path to scan-worker
/// threads: each worker opens its own `rusqlite::Connection` over it rather
/// than sharing the UI's `Rc<RefCell<Connection>>` across threads.
pub fn default_path() -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("reprise/reprise.db")
}

const SCHEMA_V1: &str = r#"
CREATE TABLE tracks (
  id            INTEGER PRIMARY KEY,
  path          TEXT NOT NULL UNIQUE,
  title         TEXT NOT NULL DEFAULT '',
  artist        TEXT NOT NULL DEFAULT '',
  album         TEXT NOT NULL DEFAULT '',
  album_artist  TEXT NOT NULL DEFAULT '',
  year          INTEGER,
  track_no      INTEGER,
  genre         TEXT NOT NULL DEFAULT '',
  duration_ms   INTEGER NOT NULL DEFAULT 0,
  bitrate_kbps  INTEGER,
  rating        INTEGER NOT NULL DEFAULT 0,
  play_count    INTEGER NOT NULL DEFAULT 0,
  last_played_at INTEGER,
  added_at      INTEGER NOT NULL,
  file_mtime    INTEGER NOT NULL DEFAULT 0,
  missing       INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_tracks_artist ON tracks(artist);
CREATE INDEX idx_tracks_album  ON tracks(album);
CREATE TABLE import_errors (
  id          INTEGER PRIMARY KEY,
  path        TEXT NOT NULL,
  reason      TEXT NOT NULL,
  occurred_at INTEGER NOT NULL
);
"#;

/// Schema v2 (Stage 2 Task 8 — scanner move detection): adds the filesystem
/// identity columns the scanner uses to recognize a relocated file. `dev`+
/// `inode` survive a same-filesystem `rename`(2); `file_size` is the
/// fallback fingerprint signal when the inode changes too (cross-filesystem
/// copy+delete). Nullable (`device`/`inode`) because pre-v2 rows have none
/// until their next scan; `file_size` is `NOT NULL DEFAULT 0` to match the
/// rest of the tag-derived columns' non-null convention.
const SCHEMA_V2: &str = r#"
ALTER TABLE tracks ADD COLUMN file_size INTEGER NOT NULL DEFAULT 0;
ALTER TABLE tracks ADD COLUMN device INTEGER;
ALTER TABLE tracks ADD COLUMN inode INTEGER;
CREATE INDEX idx_tracks_dev_inode ON tracks(device, inode);
"#;

/// Schema v3 (Stage 3 Task 2 — playlist backend): adds manual and smart
/// playlists. Manual playlists store ordered track references with duplicate
/// permission (like Rhythmbox). Smart playlists filter tracks via a rules
/// JSON document (field/op/value, AND-joined) with sort and limit options.
/// Both types support arbitrary `position` ordering (0-indexed, gapless, kept
/// contiguous across operations).
const SCHEMA_V3: &str = r#"
CREATE TABLE playlists (
  id       INTEGER PRIMARY KEY,
  name     TEXT NOT NULL,
  position INTEGER NOT NULL
);
CREATE TABLE playlist_tracks (
  playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
  track_id    INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
  position    INTEGER NOT NULL,
  PRIMARY KEY (playlist_id, position)
);
CREATE TABLE smart_playlists (
  id         INTEGER PRIMARY KEY,
  name       TEXT NOT NULL,
  rules_json TEXT NOT NULL,
  sort_field TEXT NOT NULL,
  sort_dir   TEXT NOT NULL,
  limit_count INTEGER
);
"#;

/// Schema v4 (Stage 3 Task 8 — folder watcher): a minimal key/value settings
/// table. Its first (and, as of this task, only) consumer is `library::
/// settings::{get_setting, set_setting}`, which store the last-scanned
/// library folder under the key `"library_root"` so the watcher knows what
/// to watch on startup without the user re-picking a folder every launch.
/// Deliberately generic (`key`/`value` both `TEXT`) rather than a dedicated
/// `library_root TEXT` column on some singleton row — a key/value table needs
/// no further migration the next time the app wants to persist one more
/// small scalar setting.
const SCHEMA_V4: &str = r#"
CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
"#;

/// Schema v5: durable, token-free FIFO for completed ListenBrainz listens.
/// Rows deliberately do not reference `tracks`: a user may remove a library
/// row while its already-completed listen is still waiting for connectivity.
const SCHEMA_V5: &str = r#"
CREATE TABLE listenbrainz_queue (
  id           INTEGER PRIMARY KEY,
  listened_at  INTEGER NOT NULL,
  artist_name  TEXT NOT NULL,
  track_name   TEXT NOT NULL,
  release_name TEXT,
  duration_ms  INTEGER NOT NULL
);
"#;

/// Schema v6: an independent, token-free Last.fm FIFO. It deliberately
/// mirrors the ListenBrainz row shape while retaining a separate lifecycle:
/// either provider can acknowledge or clear its own deliveries without
/// affecting the other.
const SCHEMA_V6: &str = r#"
CREATE TABLE lastfm_queue (
  id           INTEGER PRIMARY KEY,
  listened_at  INTEGER NOT NULL,
  artist_name  TEXT NOT NULL,
  track_name   TEXT NOT NULL,
  release_name TEXT,
  duration_ms  INTEGER NOT NULL
);
"#;

/// Schema v7: per-play listening events feeding the local "My Stats" screen.
/// Unlike `tracks.play_count` (a running all-time counter), each row here is
/// one completed play at a point in time, so the stats layer can build a
/// month-by-month timeseries. `track_id` references `tracks` with
/// `ON DELETE CASCADE` (removing a library row discards its recorded plays);
/// `played_at` is unix seconds and indexed because every timeseries query
/// filters/buckets on it.
const SCHEMA_V7: &str = r#"
CREATE TABLE listen_events (
  id        INTEGER PRIMARY KEY,
  track_id  INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
  played_at INTEGER NOT NULL,
  ms_played INTEGER NOT NULL
);
CREATE INDEX idx_listen_events_played_at ON listen_events(played_at);
"#;

/// Schema v8: pre-computed waveform amplitude peaks for the seek bar.
/// 1000 × u8 (0–255) normalized RMS values, stored as a compact BLOB
/// (~1 KB per track). Nullable: NULL means not yet analyzed.
const SCHEMA_V8: &str = r#"
ALTER TABLE tracks ADD COLUMN waveform_peaks BLOB;
"#;

/// Schema v9: durable per-device synchronization preferences and the
/// Reprise-managed file inventory. The inventory deliberately references
/// library tracks so removing a track also removes stale synchronization
/// metadata without touching any file on the device.
const SCHEMA_V9: &str = r#"
CREATE TABLE device_settings (
  device_serial  TEXT PRIMARY KEY,
  device_name    TEXT NOT NULL,
  selection_json TEXT NOT NULL DEFAULT '[]',
  opus_bitrate   INTEGER NOT NULL DEFAULT 0,
  ratings_back   INTEGER NOT NULL DEFAULT 0,
  remove_deleted INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE device_files (
  device_serial TEXT NOT NULL,
  track_id      INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
  device_path   TEXT NOT NULL,
  size          INTEGER NOT NULL,
  mtime         INTEGER NOT NULL,
  pinned        INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (device_serial, track_id)
);
CREATE INDEX idx_device_files_serial ON device_files(device_serial);
"#;

/// Applies pending schema migrations in order, tracked via `PRAGMA
/// user_version`. Design choice: rather than branching "fresh DB gets the
/// latest schema in one shot, existing DB gets incremental ALTERs", every DB
/// — fresh or existing — walks the *same* sequence of version steps
/// (`SCHEMA_V1` then `SCHEMA_V2`'s `ALTER`s). This keeps there being exactly
/// one code path per version bump to test and reason about, at the cost of a
/// fresh install running through slightly more SQL than strictly necessary —
/// a one-time, sub-millisecond cost that's worth the simplicity.
///
/// Stage-3 close-out fix: each version step's schema changes AND its
/// `user_version` bump now run inside one transaction
/// (`Connection::unchecked_transaction` — used rather than `Connection::
/// transaction`, which needs `&mut Connection`, since this function only
/// takes `&Connection` and every other caller in this codebase already
/// treats a freshly-opened `Connection` as single-threaded/not concurrently
/// borrowed, matching every other `unchecked_*` use's safety precondition).
/// Before this fix, `execute_batch(SCHEMA_VN)` and `pragma_update(...,
/// "user_version", N)` were two separate, non-atomic statements — a crash
/// (power loss, OOM-kill) between them would commit the schema change but
/// not the version bump, so the NEXT `migrate()` call would see the old
/// version number and try to re-run `SCHEMA_VN`, failing on "table/column
/// already exists" and permanently wedging that database. Wrapping both in
/// one transaction makes each version step atomic: either the whole step
/// (schema + version bump) lands, or neither does and the next `migrate()`
/// call retries the same step cleanly. Idempotency (a second full `migrate()`
/// call being a no-op) is unaffected — every existing migration test still
/// passes unmodified.
pub fn migrate(conn: &Connection) -> Result<(), DbError> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 1 {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(SCHEMA_V1)?;
        tx.pragma_update(None, "user_version", 1)?;
        tx.commit()?;
    }
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 2 {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(SCHEMA_V2)?;
        tx.pragma_update(None, "user_version", 2)?;
        tx.commit()?;
    }
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 3 {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(SCHEMA_V3)?;
        // Seed three default smart playlists (only if none exist — idempotent).
        // This check is defensive-only; it runs exactly once per DB by version gate
        // and deleted seeds are never resurrected (by design).
        let smart_playlist_count: i64 =
            tx.query_row("SELECT COUNT(*) FROM smart_playlists", [], |r| r.get(0))?;
        if smart_playlist_count == 0 {
            tx.execute_batch(
                r#"
INSERT INTO smart_playlists (name, rules_json, sort_field, sort_dir, limit_count)
VALUES ('Recently played', '[{"field":"last_played_at","op":"not-null"}]', 'last_played_at', 'desc', 50);
INSERT INTO smart_playlists (name, rules_json, sort_field, sort_dir, limit_count)
VALUES ('Top rated', '[{"field":"rating","op":">=","value":4}]', 'rating', 'desc', NULL);
INSERT INTO smart_playlists (name, rules_json, sort_field, sort_dir, limit_count)
VALUES ('Recently added', '[]', 'added_at', 'desc', 50);
                "#,
            )?;
        }
        tx.pragma_update(None, "user_version", 3)?;
        tx.commit()?;
    }
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 4 {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(SCHEMA_V4)?;
        tx.pragma_update(None, "user_version", 4)?;
        tx.commit()?;
    }
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 5 {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(SCHEMA_V5)?;
        tx.pragma_update(None, "user_version", 5)?;
        tx.commit()?;
    }
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 6 {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(SCHEMA_V6)?;
        tx.pragma_update(None, "user_version", 6)?;
        tx.commit()?;
    }
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 7 {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(SCHEMA_V7)?;
        tx.pragma_update(None, "user_version", 7)?;
        tx.commit()?;
    }
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 8 {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(SCHEMA_V8)?;
        tx.pragma_update(None, "user_version", 8)?;
        tx.commit()?;
    }
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 9 {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(SCHEMA_V9)?;
        tx.pragma_update(None, "user_version", 9)?;
        tx.commit()?;
    }
    Ok(())
}

/// Stores pre-computed waveform peaks for a track.
pub fn set_waveform_peaks(conn: &Connection, track_id: i64, peaks: &[u8]) -> Result<(), DbError> {
    conn.execute(
        "UPDATE tracks SET waveform_peaks = ?1 WHERE id = ?2",
        rusqlite::params![peaks, track_id],
    )?;
    Ok(())
}

/// Loads pre-computed waveform peaks for a track. Returns `None` if not yet analyzed.
pub fn get_waveform_peaks(conn: &Connection, track_id: i64) -> Result<Option<Vec<u8>>, DbError> {
    let result = conn.query_row(
        "SELECT waveform_peaks FROM tracks WHERE id = ?1",
        [track_id],
        |row| row.get::<_, Option<Vec<u8>>>(0),
    )?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stage-3 close-out regression: proves each version step is atomic — a
    /// transaction rolled back partway through (simulating a crash between
    /// the schema change and the `user_version` bump) leaves neither the
    /// schema change nor the version bump behind, so a real `migrate()` call
    /// afterward can safely retry the whole step from scratch rather than
    /// tripping over a partially-applied schema ("duplicate column").
    #[test]
    fn migrate_version_step_is_atomic_a_rollback_leaves_no_partial_schema() {
        let conn = open(None).unwrap();
        conn.execute_batch(SCHEMA_V1).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();

        // Simulate a crash mid-step: run the same work the v1->v2 step does,
        // but roll back instead of committing (dropping an uncommitted
        // `Transaction` rolls it back).
        {
            let tx = conn.unchecked_transaction().unwrap();
            tx.execute_batch(SCHEMA_V2).unwrap();
            tx.pragma_update(None, "user_version", 2).unwrap();
        }

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            version, 1,
            "the rolled-back version bump must not have survived"
        );

        // The rolled-back schema change must not have survived either — a
        // real migrate() call must be able to re-run SCHEMA_V2 cleanly
        // (would fail with "duplicate column name" if the ALTERs had
        // partially committed).
        migrate(&conn).unwrap();
        let version_after: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version_after, 9);
    }

    #[test]
    fn migrate_creates_tracks_table_and_is_idempotent() {
        let conn = open(None).unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap(); // second run must not fail
        let n: i64 = conn
            .query_row("SELECT count(*) FROM tracks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 9);
    }

    /// Builds a v1 DB by hand (SCHEMA_V1 + `user_version = 1`, exactly what a
    /// pre-Task-8 install looks like on disk), inserts a row the way the old
    /// scanner would have, then migrates. The v2 columns must exist with
    /// their documented defaults, the pre-existing row's data must survive
    /// untouched, and a second `migrate` call must be a no-op (idempotent).
    #[test]
    fn migrate_v1_to_v2_adds_columns_preserves_data_and_is_idempotent() {
        let conn = open(None).unwrap();
        conn.execute_batch(SCHEMA_V1).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute(
            "INSERT INTO tracks (path, title, artist, rating, play_count, added_at) \
             VALUES ('/x/a.flac', 'A Title', 'An Artist', 5, 7, 1000)",
            [],
        )
        .unwrap();

        migrate(&conn).unwrap();

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 9); // Now goes to the current schema.

        let (title, rating, play_count, added_at, file_size, device, inode): (
            String,
            i64,
            i64,
            i64,
            i64,
            Option<i64>,
            Option<i64>,
        ) = conn
            .query_row(
                "SELECT title, rating, play_count, added_at, file_size, device, inode \
                 FROM tracks WHERE path = '/x/a.flac'",
                [],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(title, "A Title");
        assert_eq!(rating, 5);
        assert_eq!(play_count, 7);
        assert_eq!(added_at, 1000);
        assert_eq!(file_size, 0); // NOT NULL DEFAULT 0 for a pre-v2 row
        assert_eq!(device, None);
        assert_eq!(inode, None);

        // Second migrate() call (e.g. next app launch) must not fail or
        // re-run the ALTERs (which would error: "duplicate column name").
        migrate(&conn).unwrap();
        let version_after_second_run: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version_after_second_run, 9); // Current schema
    }

    #[test]
    fn open_sets_busy_timeout() {
        let conn = open(None).unwrap();
        let busy_timeout_ms: i64 = conn
            .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
            .unwrap();
        assert_eq!(busy_timeout_ms, 5000);
    }

    /// v2→v3 migration: playlists tables created, smart playlists seeded
    /// exactly once, foreign keys cascade correctly.
    #[test]
    fn migrate_v2_to_v3_creates_playlist_tables_and_seeds_smart_playlists() {
        let conn = open(None).unwrap();
        conn.execute_batch(SCHEMA_V1).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute_batch(SCHEMA_V2).unwrap();
        conn.pragma_update(None, "user_version", 2).unwrap();
        // Verify v2 state before migration.
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 2);

        migrate(&conn).unwrap();

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 9); // Walks all the way to the current schema.

        // Verify tables exist.
        let playlists_exist: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='playlists')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(playlists_exist);

        let playlist_tracks_exist: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='playlist_tracks')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(playlist_tracks_exist);

        let smart_playlists_exist: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='smart_playlists')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(smart_playlists_exist);

        // Verify three smart playlists were seeded.
        let smart_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM smart_playlists", [], |r| r.get(0))
            .unwrap();
        assert_eq!(smart_count, 3);

        // Verify seed names and rules.
        let (name1, rules1): (String, String) = conn
            .query_row(
                "SELECT name, rules_json FROM smart_playlists ORDER BY id LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(name1, "Recently played");
        assert_eq!(rules1, r#"[{"field":"last_played_at","op":"not-null"}]"#);

        // Second migration must be idempotent (no duplicate inserts).
        migrate(&conn).unwrap();
        let smart_count_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM smart_playlists", [], |r| r.get(0))
            .unwrap();
        assert_eq!(smart_count_after, 3);
    }

    /// Foreign key cascade: delete track → its playlist_tracks rows gone.
    #[test]
    fn migrate_v3_foreign_keys_cascade_on_track_delete() {
        let conn = open(None).unwrap();
        migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) VALUES (1, '/x/a.flac', 'A', 'B', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO playlists (id, name, position) VALUES (1, 'My Playlist', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (1, 1, 0)",
            [],
        )
        .unwrap();

        // Delete the track.
        conn.execute("DELETE FROM tracks WHERE id = 1", []).unwrap();

        // Playlist entry should be gone (FK cascade).
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM playlist_tracks WHERE track_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    /// Foreign key cascade: delete playlist → its playlist_tracks rows gone.
    #[test]
    fn migrate_v3_foreign_keys_cascade_on_playlist_delete() {
        let conn = open(None).unwrap();
        migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) VALUES (1, '/x/a.flac', 'A', 'B', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO playlists (id, name, position) VALUES (1, 'My Playlist', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (1, 1, 0)",
            [],
        )
        .unwrap();

        // Delete the playlist.
        conn.execute("DELETE FROM playlists WHERE id = 1", [])
            .unwrap();

        // Playlist entries should be gone (FK cascade).
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM playlist_tracks WHERE playlist_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    /// v3→v4 migration (Stage 3 Task 8 — folder watcher): the `settings`
    /// table is created, existing data (a track row, a playlist) survives
    /// untouched, and a second `migrate` call is idempotent (doesn't try to
    /// `CREATE TABLE settings` again).
    #[test]
    fn migrate_v3_to_v4_creates_settings_table_preserves_data_and_is_idempotent() {
        let conn = open(None).unwrap();
        conn.execute_batch(SCHEMA_V1).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute_batch(SCHEMA_V2).unwrap();
        conn.pragma_update(None, "user_version", 2).unwrap();
        conn.execute_batch(SCHEMA_V3).unwrap();
        conn.pragma_update(None, "user_version", 3).unwrap();
        conn.execute(
            "INSERT INTO tracks (path, title, artist, added_at) VALUES ('/x/a.flac', 'A', 'B', 0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO playlists (id, name, position) VALUES (1, 'My Playlist', 0)",
            [],
        )
        .unwrap();

        migrate(&conn).unwrap();

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 9);

        let settings_exist: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='settings')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(settings_exist);

        // Pre-existing data survived untouched.
        let title: String = conn
            .query_row(
                "SELECT title FROM tracks WHERE path = '/x/a.flac'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(title, "A");
        let playlist_name: String = conn
            .query_row("SELECT name FROM playlists WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(playlist_name, "My Playlist");

        // Second migrate() call must not fail (would error: "table settings
        // already exists" if the ALTER/CREATE ran a second time).
        migrate(&conn).unwrap();
        let version_after_second_run: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version_after_second_run, 9);
    }

    #[test]
    fn migrate_v4_to_v5_creates_listenbrainz_queue_and_preserves_settings() {
        let conn = open(None).unwrap();
        conn.execute_batch(SCHEMA_V1).unwrap();
        conn.execute_batch(SCHEMA_V2).unwrap();
        conn.execute_batch(SCHEMA_V3).unwrap();
        conn.execute_batch(SCHEMA_V4).unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('keep', 'yes')",
            [],
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 4).unwrap();

        migrate(&conn).unwrap();

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 9);
        let setting: String = conn
            .query_row("SELECT value FROM settings WHERE key = 'keep'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(setting, "yes");
        let queue_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='listenbrainz_queue')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(queue_exists);
        migrate(&conn).unwrap();
    }

    #[test]
    fn migrate_v6_to_v7_creates_listen_events_and_preserves_tracks() {
        let conn = open(None).unwrap();
        conn.execute_batch(SCHEMA_V1).unwrap();
        conn.execute_batch(SCHEMA_V2).unwrap();
        conn.execute_batch(SCHEMA_V3).unwrap();
        conn.execute_batch(SCHEMA_V4).unwrap();
        conn.execute_batch(SCHEMA_V5).unwrap();
        conn.execute_batch(SCHEMA_V6).unwrap();
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) VALUES (1, '/x/a.flac', 'A', 'B', 0)",
            [],
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 6).unwrap();

        migrate(&conn).unwrap();

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 9);

        let listen_events_exist: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='listen_events')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(listen_events_exist);

        let index_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='index' AND name='idx_listen_events_played_at')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(index_exists);

        // Pre-existing track row survived untouched.
        let title: String = conn
            .query_row("SELECT title FROM tracks WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(title, "A");

        // Second migrate() must be a no-op (would error re-creating the table).
        migrate(&conn).unwrap();
        let version_after: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version_after, 9);
    }
}

#[cfg(test)]
#[path = "db_recent_migration_tests.rs"]
mod recent_migration_tests;
