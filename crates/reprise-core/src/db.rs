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

/// Opens a database and applies every pending schema migration before it is
/// returned to a feature. Frontends should use this boundary for worker
/// connections instead of duplicating schema-readiness details.
pub fn open_migrated(path: Option<&Path>) -> Result<Connection, DbError> {
    let conn = open(path)?;
    migrate(&conn)?;
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
/// Reprise-managed file inventory.
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

/// Schema v10 (Missing/Import-errors rebuild, Task 1.1): the first step of
/// turning "Missing files" and "Import errors" from static error logs into
/// self-healing state lists. Two design decisions drive the shape below:
///
/// (a) `missing_since IS NULL` becomes the single source of truth for "file
/// is present". The existing `missing` boolean is being retired *in favor
/// of* a timestamp rather than kept alongside one permanently: a flag plus a
/// date can drift out of sync (whoever updates one can forget the other),
/// and a planned auto-clean feature deletes rows based on how long
/// `missing_since` has been set — a row with an unclear start date is not
/// something that feature can ever be allowed to treat as safely removable.
/// `missing` itself is NOT dropped here — it stays populated (a `missing=1`
/// row now carries both `missing=1` and `missing_since` set; that
/// redundancy is intentional and temporary) because a separate later
/// migration (Task 1.3, schema v11) is responsible for dropping the column
/// once every reader has moved onto `missing_since`. `missing_reason`,
/// `mount_point` and `removed_at` are the rest of the self-healing state:
/// why a file is considered missing, which mount it was last seen under (so
/// a remount can be recognized instead of misread as a deletion), and when
/// it was confirmed gone for good. `untagged` is unrelated to the
/// missing/import-errors rebuild but piggybacks on this migration since it
/// is the same kind of small non-null tag-derived flag as the rest of the
/// `tracks` columns (`NOT NULL DEFAULT 0`, matching that column family's
/// existing convention — see `SCHEMA_V2`'s doc comment).
///
/// (b) Existing `import_errors` rows are discarded, not migrated. This
/// table is `DROP`ped and recreated with an incompatible shape (typed
/// `reason_kind`/`reason_detail` instead of one free-text `reason`, `path`
/// promoted to the primary key, `first_seen`/`last_seen`/`seen_count` for
/// recurrence tracking, and `dismissed_mtime`/`dismissed_size` for a later
/// task's "this exact broken file was already dismissed" fingerprint).
/// Unlike `tracks` rows, which carry user data (ratings, playlist
/// positions) that must survive any migration, `import_errors` rows are
/// reproducible scan state: the very next scan recreates any row that is
/// still actually failing, this time correctly typed. Migrating the old
/// rows instead would mean guessing a `reason_kind` out of free-text
/// `reason` strings never written with a fixed vocabulary — fragile string
/// parsing for state a fresh scan reconstructs for free.
///
/// Backfilled `missing=1` rows get `missing_reason = 'unknown'`, never
/// `'deleted'`: the v1 schema had no mount check, so there is no evidence
/// distinguishing "file was deleted" from "file's mount is currently
/// absent" for any row that predates this migration. Nothing downstream may
/// ever treat an `'unknown'`-reason row as safely auto-removable without
/// re-verifying the file first.
const SCHEMA_V10: &str = r#"
ALTER TABLE tracks ADD COLUMN missing_since INTEGER;
ALTER TABLE tracks ADD COLUMN missing_reason TEXT;
ALTER TABLE tracks ADD COLUMN mount_point TEXT;
ALTER TABLE tracks ADD COLUMN removed_at INTEGER;
ALTER TABLE tracks ADD COLUMN untagged INTEGER NOT NULL DEFAULT 0;
UPDATE tracks SET missing_since = strftime('%s','now'), missing_reason = 'unknown' WHERE missing = 1;
DROP TABLE import_errors;
CREATE TABLE import_errors (
  path           TEXT PRIMARY KEY,
  reason_kind    TEXT NOT NULL,
  reason_detail  TEXT NOT NULL,
  first_seen     INTEGER NOT NULL,
  last_seen      INTEGER NOT NULL,
  seen_count     INTEGER NOT NULL DEFAULT 1,
  dismissed_mtime INTEGER,
  dismissed_size  INTEGER
);
"#;

/// Schema v11 (Missing files rebuild, Task 1.3): drops the now-unused
/// `missing` boolean column from the tracks table. Design decision: the
/// boolean flag plus a timestamp are two truths for one state and can drift
/// out of sync; `missing_since IS NULL` is now the single source of truth for
/// "file is present", and a planned auto-clean feature deletes rows based on
/// how long `missing_since` has been set — a row with an unclear boolean/date
/// agreement would be unacceptable there. Schema v10 and v11 are separate
/// migrations rather than combined into one because each task's commit must
/// leave the test suite green, and a shipped migration must never be edited
/// afterwards — the column-drop gets its own version rather than being
/// retrofitted into v10.
const SCHEMA_V11: &str = r#"
ALTER TABLE tracks DROP COLUMN missing;
"#;

/// Schema v12: gives the default title-sorted library window the ordering
/// SQLite needs to stream rows instead of sorting the whole library into a
/// temporary B-tree for every 200-row window. The partial predicate exactly
/// matches the shared `queries::PRESENT` contract, so missing and tombstoned
/// rows do not enlarge this library-only index. `COLLATE NOCASE` must be part
/// of the index expression because the visible title order uses that collation
/// and SQLite cannot satisfy it from a binary-collated title index.
const SCHEMA_V12: &str = r#"
CREATE INDEX idx_tracks_present_title_nocase
ON tracks(title COLLATE NOCASE)
WHERE missing_since IS NULL AND removed_at IS NULL;
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
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 10 {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(SCHEMA_V10)?;
        tx.pragma_update(None, "user_version", 10)?;
        tx.commit()?;
    }
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 11 {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(SCHEMA_V11)?;
        tx.pragma_update(None, "user_version", 11)?;
        tx.commit()?;
    }
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 12 {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(SCHEMA_V12)?;
        tx.pragma_update(None, "user_version", 12)?;
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

/// Returns live tracks which still need waveform analysis, in stable id
/// order. SQL ownership stays in core while platform frontends only schedule
/// extraction work.
pub fn pending_waveform_tracks(conn: &Connection) -> Result<Vec<(i64, String)>, DbError> {
    let mut statement = conn.prepare(&format!(
        "SELECT id, path FROM tracks \
         WHERE waveform_peaks IS NULL AND {} ORDER BY id",
        crate::queries::PRESENT
    ))?;
    let tracks = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;
    Ok(tracks)
}

#[cfg(test)]
#[path = "db_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "db_recent_migration_tests.rs"]
mod recent_migration_tests;
