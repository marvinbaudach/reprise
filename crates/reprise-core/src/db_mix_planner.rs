use rusqlite::Connection;

const SCHEMA_V23: &str = r#"
CREATE TABLE IF NOT EXISTS mix_drafts (
  draft_id TEXT PRIMARY KEY,
  draft_json TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  status TEXT NOT NULL DEFAULT 'current' CHECK (status IN ('current', 'approved')),
  approved_playlist_id INTEGER REFERENCES playlists(id),
  idempotency_key TEXT
);
CREATE TABLE IF NOT EXISTS mix_draft_tracks (
  draft_id TEXT NOT NULL REFERENCES mix_drafts(draft_id) ON DELETE CASCADE,
  position INTEGER NOT NULL,
  track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
  source_mtime INTEGER NOT NULL,
  source_size INTEGER NOT NULL,
  PRIMARY KEY (draft_id, position)
);
CREATE INDEX IF NOT EXISTS idx_mix_drafts_expiry ON mix_drafts(status, expires_at);
"#;

// Retained as immutable schema history: the Create Similar Mix feature was
// removed and its tables are dropped by v27 (see
// `db_drop_audio_analysis_mix`), but every database still walks this v23 step.
pub(crate) fn migrate_v23(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version < 23 {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(SCHEMA_V23)?;
        tx.pragma_update(None, "user_version", 23)?;
        tx.commit()?;
    }
    Ok(())
}
