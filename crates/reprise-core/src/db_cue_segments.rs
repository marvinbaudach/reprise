//! Schema v90: a track is identified by `(path, segment_index)`, so one audio
//! file can hold the many tracks of a CUE sheet.
//!
//! `tracks.path` carried an inline `UNIQUE`, which SQLite cannot drop, so the
//! table is rebuilt. The uniqueness is `(path, segment_index)` and not
//! `(path, segment_start_ms)`: SQLite treats NULLs as distinct in a UNIQUE
//! constraint, so a nullable start column would admit duplicate whole-file rows.
//! `segment_index` is `0` for a whole file and the one-based position of the
//! track in its file otherwise.
//!
//! `library_exclusions` gains the same `segment_index`, so removing one track of
//! a CUE file from the library hides that track and not its siblings.

use std::collections::BTreeSet;

use rusqlite::{Connection, Transaction};

const VERSION: i64 = 90;

/// The rebuilt table. Every column the table has at v89 is listed, with the
/// CHECK that `artist_mbid_negative` carries; the migration test compares this
/// list against the live schema so a column cannot be dropped by accident.
const CREATE_TRACKS: &str = "
CREATE TABLE tracks_v90 (
  id                   INTEGER PRIMARY KEY,
  path                 TEXT NOT NULL,
  title                TEXT NOT NULL DEFAULT '',
  artist               TEXT NOT NULL DEFAULT '',
  album                TEXT NOT NULL DEFAULT '',
  album_artist         TEXT NOT NULL DEFAULT '',
  year                 INTEGER,
  track_no             INTEGER,
  genre                TEXT NOT NULL DEFAULT '',
  duration_ms          INTEGER NOT NULL DEFAULT 0,
  bitrate_kbps         INTEGER,
  rating               INTEGER NOT NULL DEFAULT 0,
  play_count           INTEGER NOT NULL DEFAULT 0,
  last_played_at       INTEGER,
  added_at             INTEGER NOT NULL,
  file_mtime           INTEGER NOT NULL DEFAULT 0,
  file_size            INTEGER NOT NULL DEFAULT 0,
  device               INTEGER,
  inode                INTEGER,
  waveform_peaks       BLOB,
  missing_since        INTEGER,
  missing_reason       TEXT,
  mount_point          TEXT,
  removed_at           INTEGER,
  untagged             INTEGER NOT NULL DEFAULT 0,
  artist_mbid          TEXT,
  artist_mbid_negative INTEGER NOT NULL DEFAULT 0
    CHECK (artist_mbid_negative IN (0, 1)),
  disc_no              INTEGER,
  rated_at             INTEGER,
  rg_track_gain        REAL,
  rg_track_peak        REAL,
  rg_album_gain        REAL,
  rg_album_peak        REAL,
  tag_scan_version     INTEGER NOT NULL DEFAULT 0,
  segment_index        INTEGER NOT NULL DEFAULT 0,
  segment_start_ms     INTEGER,
  segment_end_ms       INTEGER,
  cue_path             TEXT,
  cue_mtime            INTEGER,
  cue_size             INTEGER,
  UNIQUE (path, segment_index)
)";

/// The columns the old table already has, in the order both sides use.
const COPIED_COLUMNS: &str = "id, path, title, artist, album, album_artist, year, track_no, \
     genre, duration_ms, bitrate_kbps, rating, play_count, last_played_at, added_at, \
     file_mtime, file_size, device, inode, waveform_peaks, missing_since, missing_reason, \
     mount_point, removed_at, untagged, artist_mbid, artist_mbid_negative, disc_no, \
     rated_at, rg_track_gain, rg_track_peak, rg_album_gain, rg_album_peak, tag_scan_version";

const REBUILD_EXCLUSIONS: &str = "
ALTER TABLE library_exclusions ADD COLUMN segment_index INTEGER NOT NULL DEFAULT 0;
DROP INDEX IF EXISTS idx_library_exclusions_path;
DROP INDEX IF EXISTS idx_library_exclusions_identity;
CREATE UNIQUE INDEX idx_library_exclusions_path
  ON library_exclusions(path, segment_index)
  WHERE device IS NULL OR inode IS NULL;
CREATE UNIQUE INDEX idx_library_exclusions_identity
  ON library_exclusions(device, inode, segment_index)
  WHERE device IS NOT NULL AND inode IS NOT NULL;";

/// A track whose stretch of the file changes, because the sheet was edited, no
/// longer matches the analysis stored for it. `invalidate_track_render_data`
/// covers a changed file; this covers a changed cut of an unchanged one.
const INVALIDATE_SEGMENT_ANALYSIS: &str = "
CREATE TRIGGER IF NOT EXISTS invalidate_segment_render_data
AFTER UPDATE OF segment_start_ms, segment_end_ms ON tracks
WHEN OLD.segment_start_ms IS NOT NEW.segment_start_ms
  OR OLD.segment_end_ms IS NOT NEW.segment_end_ms
BEGIN
  DELETE FROM track_spectrograms WHERE track_id = NEW.id;
  DELETE FROM track_loudness WHERE track_id = NEW.id;
  UPDATE tracks SET waveform_peaks = NULL WHERE id = NEW.id;
END;";

pub(crate) fn migrate_v90(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= VERSION {
        return Ok(());
    }
    // A database whose version was wound back over an already rebuilt schema
    // (the repair path, and the tests that rewind) must not be rebuilt again:
    // the copy lists only the columns the old table had and would drop every
    // segment on the way.
    if has_column(conn, "tracks", "segment_index")? {
        let transaction = crate::db_migrations::begin_step(conn)?;
        if !has_column(&transaction, "library_exclusions", "segment_index")? {
            transaction.execute_batch(REBUILD_EXCLUSIONS)?;
        }
        transaction.execute_batch(INVALIDATE_SEGMENT_ANALYSIS)?;
        transaction.pragma_update(None, "user_version", VERSION)?;
        return transaction.commit();
    }
    // `PRAGMA foreign_keys` is silently ignored inside a transaction, and with
    // the keys on, dropping `tracks` would cascade through every table that
    // references it. So it is switched off here, outside the transaction, and
    // read back before anything is dropped.
    if !conn.is_autocommit() {
        return Err(misuse("the tracks rebuild cannot run inside a transaction"));
    }
    let foreign_keys: bool = conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
    if foreign_keys {
        conn.pragma_update(None, "foreign_keys", "OFF")?;
        let still_on: bool = conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
        if still_on {
            return Err(misuse("foreign key enforcement could not be switched off"));
        }
    }
    let rebuilt = rebuild(conn);
    if foreign_keys {
        conn.pragma_update(None, "foreign_keys", "ON")?;
    }
    rebuilt
}

fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, rusqlite::Error> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2)",
        [table, column],
        |row| row.get(0),
    )
}

fn rebuild(conn: &Connection) -> Result<(), rusqlite::Error> {
    let transaction = crate::db_migrations::begin_step(conn)?;
    let dangling_before = dangling_references(&transaction)?;
    let replay = schema_to_replay(&transaction)?;
    transaction.execute_batch(CREATE_TRACKS)?;
    transaction.execute_batch(&format!(
        "INSERT INTO tracks_v90 ({COPIED_COLUMNS}) SELECT {COPIED_COLUMNS} FROM tracks;
         DROP TABLE tracks;"
    ))?;
    rename_rebuilt_table(&transaction)?;
    for statement in replay {
        transaction.execute_batch(&statement)?;
    }
    transaction.execute_batch(REBUILD_EXCLUSIONS)?;
    transaction.execute_batch(INVALIDATE_SEGMENT_ANALYSIS)?;
    // A reference that already dangled before the rebuild, left by some older
    // version with the keys off, is not the rebuild's doing and must not keep the
    // database from opening. Only a reference the rebuild broke stops it.
    if !dangling_references(&transaction)?.is_subset(&dangling_before) {
        return Err(misuse("the tracks rebuild left dangling references"));
    }
    transaction.pragma_update(None, "user_version", VERSION)?;
    transaction.commit()
}

/// Every row whose foreign key points at nothing, as its table, rowid and the
/// number of the key it breaks.
fn dangling_references(
    transaction: &Transaction<'_>,
) -> Result<BTreeSet<(String, Option<i64>, i64)>, rusqlite::Error> {
    let mut statement =
        transaction.prepare("SELECT \"table\", rowid, fkid FROM pragma_foreign_key_check")?;
    let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
    rows.collect()
}

/// A trigger on another table can read `tracks` (`listen_events_fill_snapshot`
/// does), and between the drop and the rename it points at nothing. The modern
/// rename re-validates every trigger and would refuse, so this one statement
/// runs with the legacy rename, which leaves other objects alone. Nothing refers
/// to the temporary name, so there is no reference for it to leave stale.
fn rename_rebuilt_table(transaction: &Transaction<'_>) -> Result<(), rusqlite::Error> {
    transaction.pragma_update(None, "legacy_alter_table", "ON")?;
    let renamed = transaction.execute_batch("ALTER TABLE tracks_v90 RENAME TO tracks;");
    transaction.pragma_update(None, "legacy_alter_table", "OFF")?;
    renamed
}

/// Every index and trigger on `tracks`, exactly as the schema stores them. They
/// vanish with the table and are replayed on the new one; nothing is listed by
/// hand, so the live `invalidate_track_render_data` body is the one that
/// survives, whichever migration last wrote it.
fn schema_to_replay(transaction: &Transaction<'_>) -> Result<Vec<String>, rusqlite::Error> {
    let mut statement = transaction.prepare(
        "SELECT sql FROM sqlite_schema
         WHERE tbl_name = 'tracks' AND type IN ('index', 'trigger') AND sql IS NOT NULL
         ORDER BY type DESC, name",
    )?;
    let statements = statement.query_map([], |row| row.get(0))?;
    statements.collect()
}

fn misuse(message: &str) -> rusqlite::Error {
    rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_MISUSE),
        Some(message.to_string()),
    )
}

#[cfg(test)]
#[path = "db_cue_segments_tests.rs"]
mod tests;
