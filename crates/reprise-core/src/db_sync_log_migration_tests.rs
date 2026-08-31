use super::*;
use crate::db::Db;

#[test]
fn migrate_v81_preserves_existing_sync_events() {
    let db = Db::open_in_memory().unwrap();
    let conn = db.conn();
    conn.execute(
        "INSERT INTO sync_runs (
           id, device_serial, device_name, transfer_profile, started_at, outcome
         ) VALUES (1, 'pixel', 'Pixel', 'original', 1, 'failed')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sync_events (run_id, kind, track_id, device_path, detail)
         VALUES (1, 'failed', 1666, 'Artist/Album/Track.opus', 'copy failed')",
        [],
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 80).unwrap();

    migrate_v81(conn).unwrap();

    let event: (i64, String, Option<i64>, String, String) = conn
        .query_row(
            "SELECT run_id, kind, track_id, device_path, detail FROM sync_events",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        event,
        (
            1,
            "failed".into(),
            Some(1666),
            "Artist/Album/Track.opus".into(),
            "copy failed".into(),
        )
    );
}
