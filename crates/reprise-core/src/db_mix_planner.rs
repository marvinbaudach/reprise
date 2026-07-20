use rusqlite::Connection;

const SCHEMA_V23: &str = r#"
CREATE TABLE mix_drafts (
  draft_id TEXT PRIMARY KEY,
  draft_json TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  status TEXT NOT NULL DEFAULT 'current' CHECK (status IN ('current', 'approved')),
  approved_playlist_id INTEGER REFERENCES playlists(id),
  idempotency_key TEXT
);
CREATE TABLE mix_draft_tracks (
  draft_id TEXT NOT NULL REFERENCES mix_drafts(draft_id) ON DELETE CASCADE,
  position INTEGER NOT NULL,
  track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
  source_mtime INTEGER NOT NULL,
  source_size INTEGER NOT NULL,
  PRIMARY KEY (draft_id, position)
);
CREATE INDEX idx_mix_drafts_expiry ON mix_drafts(status, expires_at);
"#;

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

#[cfg(test)]
mod tests {
    #[test]
    fn migration_v22_to_v23_preserves_library_and_cascades_draft_positions() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) VALUES (1, '/fixture/a', 'A', 'Artist', 1)",
            [],
        )
        .unwrap();
        conn.execute_batch(
            "DROP TABLE mix_draft_tracks; DROP TABLE mix_drafts; PRAGMA user_version = 22;",
        )
        .unwrap();
        super::migrate_v23(&conn).unwrap();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 23);
        let title: String = conn
            .query_row("SELECT title FROM tracks WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(title, "A");
        super::migrate_v23(&conn).unwrap();
    }
}
