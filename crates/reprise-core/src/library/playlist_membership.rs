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

    // A request that adds nothing (every id is already a member) settles before
    // any transaction opens, so it never queues for the write lock.
    if new_members(conn, playlist_id, track_ids)?.is_empty() {
        return Ok(0);
    }

    // IMMEDIATE: the existing members are read before the insert (see
    // `events::in_txn_immediate`). The read is repeated under the lock; that one
    // is authoritative.
    let tx = crate::events::immediate_transaction(conn)?;
    let unique = new_members(&tx, playlist_id, track_ids)?;
    if unique.is_empty() {
        tx.commit()?;
        return Ok(0);
    }

    let max_position = tx.query_row(
        "SELECT COALESCE(MAX(position), -1) FROM playlist_tracks WHERE playlist_id=?1",
        [playlist_id],
        |row| row.get::<_, i64>(0),
    )?;
    {
        let mut statement = tx.prepare_cached(
            "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (?1, ?2, ?3)",
        )?;
        for (offset, track_id) in unique.iter().enumerate() {
            statement.execute(params![
                playlist_id,
                track_id,
                max_position + 1 + offset as i64
            ])?;
        }
    }
    let inserted = unique.len() as u32;
    tx.commit()?;
    Ok(inserted)
}

/// The requested ids that are not yet members, in request order, each once.
fn new_members(
    conn: &rusqlite::Connection,
    playlist_id: i64,
    track_ids: &[i64],
) -> Result<Vec<i64>, rusqlite::Error> {
    let existing = {
        let mut statement =
            conn.prepare_cached("SELECT track_id FROM playlist_tracks WHERE playlist_id=?1")?;
        let ids = statement
            .query_map([playlist_id], |row| row.get::<_, i64>(0))?
            .collect::<Result<HashSet<_>, _>>()?;
        ids
    };
    let mut seen = HashSet::new();
    Ok(track_ids
        .iter()
        .copied()
        .filter(|track_id| seen.insert(*track_id) && !existing.contains(track_id))
        .collect())
}

#[cfg(test)]
mod tests {
    use crate::db::Db;
    use rusqlite::params;
    use rusqlite::trace::{TraceEvent, TraceEventCodes};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static MEMBERSHIP_SELECTS: AtomicUsize = AtomicUsize::new(0);

    fn count_membership_selects(event: TraceEvent<'_>) {
        let TraceEvent::Stmt(statement, _expanded) = event else {
            return;
        };
        let sql = statement.sql();
        if sql.contains("SELECT EXISTS(SELECT 1 FROM playlist_tracks")
            || sql.contains("SELECT track_id FROM playlist_tracks WHERE playlist_id")
        {
            MEMBERSHIP_SELECTS.fetch_add(1, Ordering::SeqCst);
        }
    }

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

        MEMBERSHIP_SELECTS.store(0, Ordering::SeqCst);
        db.conn().trace_v2(
            TraceEventCodes::SQLITE_TRACE_STMT,
            Some(count_membership_selects),
        );
        let inserted = super::add_unique_tracks(&db, playlist_id, &[2, 3, 3, 4]).unwrap();
        db.conn().trace_v2(TraceEventCodes::empty(), None);
        assert_eq!(inserted, 2);
        // One set read to settle a no-op before any transaction opens, one under
        // the write lock — never one per requested id.
        assert_eq!(MEMBERSHIP_SELECTS.load(Ordering::SeqCst), 2);

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
