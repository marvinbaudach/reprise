use super::*;

fn index_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
        [name],
        |row| row.get::<_, i64>(0),
    )
    .unwrap()
        > 0
}

fn create_v45_sync_events(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE sync_events (
           run_id       INTEGER NOT NULL,
           kind         TEXT NOT NULL
             CHECK (kind IN (
               'skipped','failed','deleted',
               'conversion_fallback','playlist_write_failed'
             )),
           track_id     INTEGER,
           device_path  TEXT NOT NULL,
           detail       TEXT NOT NULL
         );
         CREATE INDEX idx_sync_events_run ON sync_events(run_id);",
    )
    .unwrap();
}

#[test]
fn migrate_v81_preserves_existing_sync_events() {
    let conn = Connection::open_in_memory().unwrap();
    create_v45_sync_events(&conn);
    conn.execute(
        "INSERT INTO sync_events (run_id, kind, track_id, device_path, detail)
         VALUES (1, 'failed', 1666, 'Artist/Album/Track.opus', 'copy failed')",
        [],
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 80).unwrap();

    migrate_v81(&conn).unwrap();

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
    assert!(index_exists(&conn, "idx_sync_events_run"));
    conn.execute(
        "INSERT INTO sync_events (run_id, kind, track_id, device_path, detail)
         VALUES (1, 'analysis_failed', 1667, 'Artist/Album/Track.reprise-analysis',
                 'analysis copy failed')",
        [],
    )
    .unwrap();
}

#[test]
fn migrate_v81_repairs_the_old_check_at_the_current_version() {
    let conn = Connection::open_in_memory().unwrap();
    create_v45_sync_events(&conn);
    conn.pragma_update(None, "user_version", 81).unwrap();

    migrate_v81(&conn).unwrap();

    conn.execute(
        "INSERT INTO sync_events (run_id, kind, track_id, device_path, detail)
         VALUES (1, 'analysis_failed', 1667, 'Artist/Album/Track.reprise-analysis',
                 'analysis copy failed')",
        [],
    )
    .unwrap();
}

#[test]
fn migrate_v45_declares_the_current_analysis_failure_kind() {
    let conn = Connection::open_in_memory().unwrap();
    migrate_v45(&conn).unwrap();

    conn.execute(
        "INSERT INTO sync_events (run_id, kind, track_id, device_path, detail)
         VALUES (1, 'analysis_failed', 1667, 'Artist/Album/Track.reprise-analysis',
                 'analysis copy failed')",
        [],
    )
    .unwrap();
}
