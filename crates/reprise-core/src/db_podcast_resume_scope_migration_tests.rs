//! Schema v78: one-time cleanup of unsupported podcast resume positions.

use rusqlite::{params, Connection};

fn schema(conn: &Connection) {
    conn.execute_batch(
        "CREATE TABLE podcast_subscriptions (
           id INTEGER PRIMARY KEY,
           kind TEXT NOT NULL
         );
         CREATE TABLE podcast_episodes (
           id INTEGER PRIMARY KEY,
           subscription_id INTEGER NOT NULL,
           duration_secs INTEGER,
           position_ms INTEGER NOT NULL DEFAULT 0
         );",
    )
    .unwrap();
}

fn subscription(conn: &Connection, id: i64, kind: &str) {
    conn.execute(
        "INSERT INTO podcast_subscriptions (id, kind) VALUES (?1, ?2)",
        params![id, kind],
    )
    .unwrap();
}

fn episode(
    conn: &Connection,
    id: i64,
    subscription_id: i64,
    duration_secs: Option<i64>,
    position_ms: i64,
) {
    conn.execute(
        "INSERT INTO podcast_episodes
           (id, subscription_id, duration_secs, position_ms)
         VALUES (?1, ?2, ?3, ?4)",
        params![id, subscription_id, duration_secs, position_ms],
    )
    .unwrap();
}

fn position(conn: &Connection, episode_id: i64) -> i64 {
    conn.query_row(
        "SELECT position_ms FROM podcast_episodes WHERE id = ?1",
        [episode_id],
        |row| row.get(0),
    )
    .unwrap()
}

#[test]
fn v78_clears_youtube_and_short_rss_resume_but_keeps_long_rss_position() {
    let conn = Connection::open_in_memory().unwrap();
    schema(&conn);
    subscription(&conn, 1, "youtube");
    subscription(&conn, 2, "rss");
    episode(&conn, 11, 1, Some(3_600), 1_200_000);
    episode(&conn, 12, 2, Some(599), 120_000);
    episode(&conn, 13, 2, Some(3_600), 600_000);
    episode(&conn, 14, 2, None, 300_000);
    episode(&conn, 15, 2, Some(600), 240_000);
    conn.pragma_update(None, "user_version", 77).unwrap();

    super::migrate_v78(&conn).unwrap();

    assert_eq!(position(&conn, 11), 0, "YouTube never keeps resume");
    assert_eq!(position(&conn, 12), 0, "short RSS never keeps resume");
    assert_eq!(
        position(&conn, 13),
        600_000,
        "long RSS is the control arm and must keep its position"
    );
    assert_eq!(
        position(&conn, 14),
        300_000,
        "unknown duration counts as long and must keep its position"
    );
    assert_eq!(
        position(&conn, 15),
        240_000,
        "the exact minimum duration must keep its position"
    );
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 78);
}

#[test]
fn v78_does_not_reapply_after_the_version_was_recorded() {
    let conn = Connection::open_in_memory().unwrap();
    schema(&conn);
    subscription(&conn, 1, "youtube");
    episode(&conn, 11, 1, Some(3_600), 1_200_000);
    conn.pragma_update(None, "user_version", 78).unwrap();

    super::migrate_v78(&conn).unwrap();

    assert_eq!(position(&conn, 11), 1_200_000);
}
