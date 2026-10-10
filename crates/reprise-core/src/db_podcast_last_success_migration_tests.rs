//! Schema v92: `podcast_subscriptions.last_success_at`.

use rusqlite::Connection;

/// The schema as it was at v91: baseline plus every migration before this one.
fn at_v91() -> Connection {
    let conn = crate::db::open(None).unwrap();
    crate::db_schema_baseline::migrate_baseline(
        &conn,
        false,
        std::path::Path::new("."),
        std::path::Path::new("."),
    )
    .unwrap();
    crate::db_migrations::run_migrations_through(&conn, 91).unwrap();
    conn
}

fn subscription(conn: &Connection, id: i64, outcome: Option<&str>, last_fetch_at: Option<i64>) {
    conn.execute(
        "INSERT INTO podcast_subscriptions
           (id, kind, feed_url, title, auto_download, added_at, last_fetch_at, last_outcome)
         VALUES (?1, 'rss', ?2, 'Source', 0, 1, ?3, ?4)",
        rusqlite::params![
            id,
            format!("https://example.test/{id}"),
            last_fetch_at,
            outcome
        ],
    )
    .unwrap();
}

fn last_success_at(conn: &Connection, id: i64) -> Option<i64> {
    conn.query_row(
        "SELECT last_success_at FROM podcast_subscriptions WHERE id = ?1",
        [id],
        |row| row.get(0),
    )
    .unwrap()
}

#[test]
fn v92_backfills_the_last_success_only_from_successful_outcomes() {
    let conn = at_v91();
    subscription(&conn, 1, Some("ok"), Some(100));
    subscription(&conn, 2, Some("not_modified"), Some(200));
    subscription(&conn, 3, Some("failed"), Some(300));
    subscription(&conn, 4, None, None);

    crate::db::migrate_connection(&conn).unwrap();

    assert_eq!(last_success_at(&conn, 1), Some(100));
    assert_eq!(last_success_at(&conn, 2), Some(200));
    assert_eq!(last_success_at(&conn, 3), None);
    assert_eq!(last_success_at(&conn, 4), None);
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, crate::db::SUPPORTED_SCHEMA_VERSION);
}

#[test]
fn v92_is_a_no_op_on_a_database_that_already_has_the_column() {
    let conn = at_v91();
    crate::db::migrate_connection(&conn).unwrap();
    subscription(&conn, 1, Some("ok"), Some(100));
    conn.pragma_update(None, "user_version", 91).unwrap();

    crate::db::migrate_connection(&conn).unwrap();

    assert_eq!(last_success_at(&conn, 1), Some(100));
}
