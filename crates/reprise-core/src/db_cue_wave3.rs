//! Schema v91: an excluded CUE segment keeps its identity, and a track whose
//! render data could not be produced says so.
//!
//! `library_exclusions` gains the excluded segment's start and title, so a
//! sheet edit that shifts positions still hides the track the user removed and
//! not whichever track moved into its place. It also gains the path, mtime and
//! size of the sheet that segment came from, so the scanner can recognise a
//! sheet whose every segment is excluded as already applied. All five are NULL
//! for a whole-file exclusion and for every row written before this version.
//!
//! `render_data_failures` records an analysis that failed, against the same
//! source fingerprint and format version a stored spectrogram carries, so a
//! track that cannot be decoded is not retried on every pass until its file
//! or the analysis format changes. The row goes with its track.
//!
//! v91 also extends both invalidation triggers, `invalidate_track_render_data`
//! (a changed file) and `invalidate_segment_render_data` (a changed cut of an
//! unchanged one), so the marker goes with the analysis it stands in for: a
//! failure recorded against the old audio says nothing about the new.

use rusqlite::Connection;

const VERSION: i64 = 91;

const EXCLUSION_IDENTITY: &str = "
ALTER TABLE library_exclusions ADD COLUMN segment_start_ms INTEGER;
ALTER TABLE library_exclusions ADD COLUMN segment_title TEXT;
ALTER TABLE library_exclusions ADD COLUMN cue_path TEXT;
ALTER TABLE library_exclusions ADD COLUMN cue_mtime INTEGER;
ALTER TABLE library_exclusions ADD COLUMN cue_size INTEGER;";

/// The fingerprint columns and their CHECKs are those of `track_spectrograms`.
const RENDER_DATA_FAILURES: &str = "
CREATE TABLE IF NOT EXISTS render_data_failures (
  track_id       INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
  source_mtime   INTEGER NOT NULL CHECK (source_mtime >= 0),
  source_size    INTEGER NOT NULL CHECK (source_size >= 0),
  source_device  INTEGER,
  source_inode   INTEGER,
  format_version INTEGER NOT NULL CHECK (format_version > 0),
  reason         TEXT NOT NULL,
  failed_at      INTEGER NOT NULL
);";

/// Both bodies are the live ones from v89 and v90, plus the failure marker.
/// SQLite cannot alter a trigger, so each is dropped and created again.
const INVALIDATE_RENDER_DATA: &str = "
DROP TRIGGER IF EXISTS invalidate_track_render_data;
CREATE TRIGGER invalidate_track_render_data
AFTER UPDATE OF file_mtime, file_size, device, inode ON tracks
WHEN OLD.file_mtime IS NOT NEW.file_mtime
  OR OLD.file_size IS NOT NEW.file_size
  OR OLD.device IS NOT NEW.device
  OR OLD.inode IS NOT NEW.inode
BEGIN
  DELETE FROM track_spectrograms WHERE track_id = NEW.id;
  DELETE FROM track_loudness WHERE track_id = NEW.id;
  DELETE FROM render_data_failures WHERE track_id = NEW.id;
  UPDATE tracks SET waveform_peaks = NULL WHERE id = NEW.id;
END;

DROP TRIGGER IF EXISTS invalidate_segment_render_data;
CREATE TRIGGER invalidate_segment_render_data
AFTER UPDATE OF segment_start_ms, segment_end_ms ON tracks
WHEN OLD.segment_start_ms IS NOT NEW.segment_start_ms
  OR OLD.segment_end_ms IS NOT NEW.segment_end_ms
BEGIN
  DELETE FROM track_spectrograms WHERE track_id = NEW.id;
  DELETE FROM track_loudness WHERE track_id = NEW.id;
  DELETE FROM render_data_failures WHERE track_id = NEW.id;
  UPDATE tracks SET waveform_peaks = NULL WHERE id = NEW.id;
END;";

pub(crate) fn migrate_v91(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= VERSION {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    // A version wound back over this schema (the repair path, and the tests
    // that rewind) already has the columns, and SQLite has no
    // `ADD COLUMN IF NOT EXISTS`.
    if !has_column(&transaction, "library_exclusions", "segment_start_ms")? {
        transaction.execute_batch(EXCLUSION_IDENTITY)?;
    }
    transaction.execute_batch(RENDER_DATA_FAILURES)?;
    transaction.execute_batch(INVALIDATE_RENDER_DATA)?;
    transaction.pragma_update(None, "user_version", VERSION)?;
    transaction.commit()
}

fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, rusqlite::Error> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2)",
        [table, column],
        |row| row.get(0),
    )
}

#[cfg(test)]
#[path = "db_cue_wave3_migration_tests.rs"]
mod tests;
