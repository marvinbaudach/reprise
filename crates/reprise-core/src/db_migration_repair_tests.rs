//! Repair paths for databases whose `user_version` was stamped by a
//! parallel branch with a different meaning for the same number.

use super::recent_migration_tests::open_v9_database;
use super::*;

/// A database stamped `user_version = 13` by a *different* branch's v13 (the
/// network opt-in grandfathering) never received this branch's title index.
/// Migration must repair such a database instead of aborting with
/// `no such index: idx_tracks_present_title_nocase`.
#[test]
fn migrate_repairs_a_foreign_v13_without_the_title_index() {
    let conn = open_v9_database();
    conn.execute_batch(SCHEMA_V10).unwrap();
    conn.pragma_update(None, "user_version", 10).unwrap();
    conn.execute_batch(SCHEMA_V11).unwrap();
    conn.pragma_update(None, "user_version", 11).unwrap();
    conn.execute_batch(SCHEMA_V12).unwrap();
    conn.pragma_update(None, "user_version", 12).unwrap();
    // Deliberately NOT SCHEMA_V13: the foreign v13 only wrote settings rows.
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('module.cover_download.enabled', '1')",
        [],
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 13).unwrap();
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, album, track_no, added_at) \
         VALUES (1, '/x/1.flac', 'Solo', 'Artist', 'alpha', 1, 0)",
        [],
    )
    .unwrap();

    migrate(&conn).unwrap();

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 17);
    let indexes: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'index' \
             AND name IN ('idx_tracks_present_title_nocase', 'idx_tracks_present_album_order', \
                          'idx_listen_events_track_played')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(indexes, 3, "all repaired and current indexes exist");
    let module: String = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'module.cover_download.enabled'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        module, "1",
        "the foreign branch's settings survive the repair"
    );
}
