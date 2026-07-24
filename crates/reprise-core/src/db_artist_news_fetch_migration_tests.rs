use rusqlite::Connection;

fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |row| row.get::<_, i64>(0),
    )
    .unwrap()
        > 0
}

#[test]
fn v30_creates_ledger_and_backfills_from_new_releases() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO new_releases (release_group_mbid, artist_name, artist_mbid, title, \
         release_type, first_release_date, fetched_at, fallback_accent, first_seen) \
         VALUES ('rg-1', ' Pink Floyd ', 'mbid-1', 'A', 'Album', '2026-07-01', 500, '#3584E4', 500)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO new_releases (release_group_mbid, artist_name, artist_mbid, title, \
         release_type, first_release_date, fetched_at, fallback_accent, first_seen) \
         VALUES ('rg-2', 'PINK FLOYD', 'mbid-1', 'B', 'Album', '2026-07-02', 900, '#3584E4', 900)",
        [],
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 29).unwrap();
    conn.execute("DROP TABLE IF EXISTS artist_news_fetch", []).unwrap();

    crate::db_artist_news_fetch::migrate_v30(&conn).unwrap();

    assert!(table_exists(&conn, "artist_news_fetch"));
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 30);

    let (key, mbid, attempt, outcome, found): (String, Option<String>, i64, String, i64) = conn
        .query_row(
            "SELECT artist_key, artist_mbid, last_attempt_at, last_outcome, releases_found \
             FROM artist_news_fetch",
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
    assert_eq!(key, "pink floyd");
    assert_eq!(mbid.as_deref(), Some("mbid-1"));
    assert_eq!(attempt, 900);
    assert_eq!(outcome, "ok");
    assert_eq!(found, 2);
}

#[test]
fn v30_is_idempotent() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    crate::db_artist_news_fetch::migrate_v30(&conn).unwrap();
    crate::db_artist_news_fetch::migrate_v30(&conn).unwrap();
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 30);
}
