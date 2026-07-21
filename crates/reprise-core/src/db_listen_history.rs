//! Schema migration that turns listens into durable historical facts.

use rusqlite::Connection;

const SCHEMA_V24: &str = r#"
CREATE TABLE listen_events_v23 (
  id           INTEGER PRIMARY KEY,
  track_id     INTEGER NOT NULL,
  played_at    INTEGER NOT NULL,
  ms_played    INTEGER NOT NULL,
  title        TEXT NOT NULL DEFAULT '',
  artist       TEXT NOT NULL DEFAULT '',
  album        TEXT NOT NULL DEFAULT '',
  album_artist TEXT NOT NULL DEFAULT '',
  genre        TEXT NOT NULL DEFAULT '',
  duration_ms  INTEGER NOT NULL DEFAULT 0,
  path         TEXT NOT NULL DEFAULT '',
  artist_mbid  TEXT
);
INSERT INTO listen_events_v23
  (id, track_id, played_at, ms_played, title, artist, album, album_artist,
   genre, duration_ms, path, artist_mbid)
SELECT le.id, le.track_id, le.played_at, le.ms_played,
       COALESCE(t.title, ''), COALESCE(t.artist, ''), COALESCE(t.album, ''),
       COALESCE(t.album_artist, ''), COALESCE(t.genre, ''),
       COALESCE(t.duration_ms, 0), COALESCE(t.path, ''), t.artist_mbid
FROM listen_events le
LEFT JOIN tracks t ON t.id = le.track_id;
DROP TABLE listen_events;
ALTER TABLE listen_events_v23 RENAME TO listen_events;
CREATE INDEX idx_listen_events_played_at ON listen_events(played_at);
CREATE INDEX idx_listen_events_track_played ON listen_events(track_id, played_at);
CREATE TRIGGER listen_events_fill_snapshot
AFTER INSERT ON listen_events
WHEN NEW.title = '' AND NEW.artist = '' AND NEW.album = ''
 AND NEW.album_artist = '' AND NEW.genre = '' AND NEW.duration_ms = 0
 AND NEW.path = '' AND NEW.artist_mbid IS NULL
 AND EXISTS(SELECT 1 FROM tracks WHERE id = NEW.track_id)
BEGIN
  UPDATE listen_events SET
    title = (SELECT title FROM tracks WHERE id = NEW.track_id),
    artist = (SELECT artist FROM tracks WHERE id = NEW.track_id),
    album = (SELECT album FROM tracks WHERE id = NEW.track_id),
    album_artist = (SELECT album_artist FROM tracks WHERE id = NEW.track_id),
    genre = (SELECT genre FROM tracks WHERE id = NEW.track_id),
    duration_ms = (SELECT duration_ms FROM tracks WHERE id = NEW.track_id),
    path = (SELECT path FROM tracks WHERE id = NEW.track_id),
    artist_mbid = (SELECT artist_mbid FROM tracks WHERE id = NEW.track_id)
  WHERE id = NEW.id;
END;
"#;

pub(crate) fn migrate_v24(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 24 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(SCHEMA_V24)?;
    transaction.pragma_update(None, "user_version", 24)?;
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use crate::library::stats_screen::ListenEventSnapshot;

    #[test]
    fn browse_6_track_deletion_keeps_listen_and_its_metadata_snapshot() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks
             (id,path,title,artist,album,album_artist,genre,duration_ms,added_at)
             VALUES (1,'/music/blue.flac','River','Joni','Blue','Joni','Folk',240000,1)",
            [],
        )
        .unwrap();
        let snapshot = ListenEventSnapshot {
            title: "River".into(),
            artist: "Joni".into(),
            album: "Blue".into(),
            album_artist: "Joni".into(),
            genre: "Folk".into(),
            duration_ms: 240_000,
            path: "/music/blue.flac".into(),
            artist_mbid: None,
        };
        crate::library::stats_screen::record_listen_event(&conn, 1, 100, 200_000, &snapshot)
            .unwrap();

        conn.execute("DELETE FROM tracks WHERE id=1", []).unwrap();

        let snapshot: (i64, String, String, String) = conn
            .query_row(
                "SELECT track_id,title,artist,album FROM listen_events WHERE id=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(snapshot, (1, "River".into(), "Joni".into(), "Blue".into()));
    }

    #[test]
    fn history_2_deleted_during_playback_records_the_owned_snapshot() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        let snapshot = ListenEventSnapshot {
            title: "River".into(),
            artist: "Joni".into(),
            album: "Blue".into(),
            album_artist: "Joni".into(),
            genre: "Folk".into(),
            duration_ms: 240_000,
            path: "/music/blue.flac".into(),
            artist_mbid: Some("artist-id".into()),
        };

        crate::library::stats_screen::record_listen_event(&conn, 1, 100, 200_000, &snapshot)
            .unwrap();

        let stored: (String, String, i64, Option<String>) = conn
            .query_row(
                "SELECT title, genre, duration_ms, artist_mbid FROM listen_events WHERE id=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            stored,
            (
                "River".into(),
                "Folk".into(),
                240_000,
                Some("artist-id".into())
            )
        );
    }

    #[test]
    fn history_5_v22_upgrade_backfills_existing_listens_and_is_idempotent() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "DROP TRIGGER listen_events_fill_snapshot;
             DROP TABLE listen_events;
             CREATE TABLE listen_events (
               id INTEGER PRIMARY KEY,
               track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
               played_at INTEGER NOT NULL,
               ms_played INTEGER NOT NULL
             );
             CREATE INDEX idx_listen_events_played_at ON listen_events(played_at);
             CREATE INDEX idx_listen_events_track_played ON listen_events(track_id, played_at);
             PRAGMA user_version = 23;
             INSERT INTO tracks
               (id,path,title,artist,album,album_artist,genre,duration_ms,added_at)
             VALUES
               (1,'/music/blue.flac','River','Joni','Blue','Joni','Folk',240000,1);
             INSERT INTO listen_events (track_id,played_at,ms_played)
             VALUES (1,100,200000);",
        )
        .unwrap();

        super::migrate_v24(&conn).unwrap();
        super::migrate_v24(&conn).unwrap();

        let stored: (i64, String, String, i64) = conn
            .query_row(
                "SELECT track_id,title,genre,duration_ms FROM listen_events",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(stored, (1, "River".into(), "Folk".into(), 240_000));
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 24);
    }
}
