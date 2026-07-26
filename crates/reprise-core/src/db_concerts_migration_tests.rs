use rusqlite::{params, Connection, ErrorCode};

fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |row| row.get::<_, i64>(0),
    )
    .unwrap()
        > 0
}

fn index_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
        [name],
        |row| row.get::<_, i64>(0),
    )
    .unwrap()
        > 0
}

fn insert_event(conn: &Connection, id: i64, dedupe_key: &str) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO concert_events (
            id, artist_key, artist_name, starts_at, date_key, venue, city,
            provider, fetched_at, dedupe_key
         ) VALUES (?1, 'artist', 'Artist', '2026-10-17T19:00:00',
                   '2026-10-17', 'Zenith', 'Munich', 'bandsintown', 42, ?2)",
        params![id, dedupe_key],
    )
}

#[test]
fn v31_creates_concert_ledger_events_and_indexes() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn.pragma_update(None, "user_version", 30).unwrap();
    conn.execute("DROP TABLE IF EXISTS concert_events", [])
        .unwrap();
    conn.execute("DROP TABLE IF EXISTS concert_artists", [])
        .unwrap();

    crate::db_concerts::migrate_v31(&conn).unwrap();

    assert!(table_exists(&conn, "concert_artists"));
    assert!(table_exists(&conn, "concert_events"));
    assert!(index_exists(&conn, "idx_concert_events_date"));
    assert!(index_exists(&conn, "idx_concert_events_artist"));
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 31);

    insert_event(&conn, 1, "2026-10-17|munich|zenith").unwrap();
    let duplicate = insert_event(&conn, 2, "2026-10-17|munich|zenith").unwrap_err();
    assert_eq!(
        duplicate.sqlite_error_code(),
        Some(ErrorCode::ConstraintViolation)
    );
}

#[test]
fn v31_is_idempotent() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();

    crate::db_concerts::migrate_v31(&conn).unwrap();
    crate::db_concerts::migrate_v31(&conn).unwrap();

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 31);
}

#[test]
fn supported_schema_version_is_v31() {
    assert_eq!(crate::db::SUPPORTED_SCHEMA_VERSION, 31);
}
