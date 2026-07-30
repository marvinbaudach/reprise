use std::collections::HashSet;

use rusqlite::params;

use crate::db::Db;

/// Appends only track ids that are not already members of the playlist.
/// Repeated ids in the same request are also inserted once. The lower-level
/// `playlists::add_tracks` intentionally keeps duplicate-preserving import
/// semantics; interactive UI additions use this stricter operation.
pub fn add_unique_tracks(
    db: &Db,
    playlist_id: i64,
    track_ids: &[i64],
) -> Result<u32, rusqlite::Error> {
    let conn = db.conn();
    if track_ids.is_empty() {
        return Ok(0);
    }

    let tx = conn.unchecked_transaction()?;
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for &track_id in track_ids {
        if !seen.insert(track_id) {
            continue;
        }
        let exists = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM playlist_tracks \
             WHERE playlist_id=?1 AND track_id=?2)",
            params![playlist_id, track_id],
            |row| row.get::<_, bool>(0),
        )?;
        if !exists {
            unique.push(track_id);
        }
    }

    if unique.is_empty() {
        tx.commit()?;
        return Ok(0);
    }

    let max_position = tx.query_row(
        "SELECT COALESCE(MAX(position), -1) FROM playlist_tracks WHERE playlist_id=?1",
        [playlist_id],
        |row| row.get::<_, i64>(0),
    )?;
    for (offset, track_id) in unique.iter().enumerate() {
        tx.execute(
            "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (?1, ?2, ?3)",
            params![playlist_id, track_id, max_position + 1 + offset as i64],
        )?;
    }
    let inserted = unique.len() as u32;
    tx.commit()?;
    Ok(inserted)
}

#[cfg(test)]
mod tests {
    use crate::db::Db;
    use rusqlite::params;

    fn seeded_db() -> Db {
        let db = Db::open_in_memory().unwrap();
        for id in 1..=4 {
            db.conn()
                .execute(
                    "INSERT INTO tracks (id, path, title, artist, added_at) \
                 VALUES (?1, ?2, ?3, '', 0)",
                    params![id, format!("/x/{id}.flac"), format!("Track {id}")],
                )
                .unwrap();
        }
        db
    }

    #[test]
    fn interactive_add_skips_existing_and_repeated_track_ids() {
        let db = seeded_db();
        let playlist_id = crate::library::playlists::create(&db, "P").unwrap();
        crate::library::playlists::add_tracks(&db, playlist_id, &[1, 2]).unwrap();

        let inserted = super::add_unique_tracks(&db, playlist_id, &[2, 3, 3, 4]).unwrap();
        assert_eq!(inserted, 2);

        let ids = db
            .conn()
            .prepare(
                "SELECT track_id FROM playlist_tracks \
                 WHERE playlist_id=?1 ORDER BY position",
            )
            .unwrap()
            .query_map([playlist_id], |row| row.get::<_, i64>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(ids, vec![1, 2, 3, 4]);
    }

    #[test]
    fn interactive_add_rolls_back_when_any_track_id_is_invalid() {
        let db = seeded_db();
        let playlist_id = crate::library::playlists::create(&db, "P").unwrap();
        assert!(super::add_unique_tracks(&db, playlist_id, &[1, 99]).is_err());
        let count: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM playlist_tracks WHERE playlist_id=?1",
                [playlist_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
}
