//! Atomic manual-playlist deletion and position compaction.

use rusqlite::{params, Connection, OptionalExtension};

/// Deletes a playlist only while its `(id, name)` identity still matches,
/// cascades its track memberships, and closes the removed playlist's
/// position gap atomically. Returns whether a row was actually deleted:
/// `Ok(false)` means the `(id, name)` no longer matched — a stale dialog
/// request whose target was concurrently renamed or removed. That is a
/// successful no-op, but the caller must not report it as a deletion (the
/// UI would otherwise claim "Playlist deleted" when nothing was).
pub fn delete(conn: &Connection, id: i64, expected_name: &str) -> Result<bool, rusqlite::Error> {
    let tx = conn.unchecked_transaction()?;
    let position = tx
        .query_row(
            "SELECT position FROM playlists WHERE id = ?1 AND name = ?2",
            params![id, expected_name],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    let Some(position) = position else {
        tx.commit()?;
        return Ok(false);
    };
    tx.execute(
        "DELETE FROM playlists WHERE id = ?1 AND name = ?2",
        params![id, expected_name],
    )?;
    tx.execute(
        "UPDATE playlists SET position = position - 1 WHERE position > ?1",
        params![position],
    )?;
    tx.commit()?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::playlists;

    #[test]
    fn delete_keeps_tracks_and_compacts_remaining_playlists() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) \
             VALUES (1, '/x/track.flac', 'Track', 'Artist', 1)",
            [],
        )
        .unwrap();
        let first = playlists::create(&conn, "First").unwrap();
        let deleted = playlists::create(&conn, "To Delete").unwrap();
        let last = playlists::create(&conn, "Last").unwrap();
        conn.execute(
            "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (?1, 1, 0)",
            params![deleted],
        )
        .unwrap();

        assert!(
            delete(&conn, deleted, "To Delete").unwrap(),
            "a real delete reports true"
        );

        let mut statement = conn
            .prepare("SELECT id, position FROM playlists ORDER BY position")
            .unwrap();
        let remaining = statement
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(remaining, vec![(first, 0), (last, 1)]);
        let track_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM tracks", [], |row| row.get(0))
            .unwrap();
        let membership_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM playlist_tracks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(track_count, 1);
        assert_eq!(membership_count, 0);
    }

    #[test]
    fn stale_identity_is_a_no_op_and_reports_false() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        let id = playlists::create(&conn, "Renamed").unwrap();

        // The dialog captured the old name; a concurrent rename changed it.
        assert!(
            !delete(&conn, id, "Old name").unwrap(),
            "a stale (id, name) request deletes nothing and reports false"
        );
        let still_there: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM playlists WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(still_there, 1, "the live playlist survives the stale request");
    }
}
