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
    crate::db::migrate_connection(&conn).unwrap();
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
    crate::db::migrate_connection(&conn).unwrap();

    crate::db_concerts::migrate_v31(&conn).unwrap();
    crate::db_concerts::migrate_v31(&conn).unwrap();

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, crate::db::SUPPORTED_SCHEMA_VERSION);
}

#[test]
fn v73_preserves_cached_concerts_and_defaults_availability_to_unknown() {
    let conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "user_version", 30).unwrap();
    crate::db_concerts::migrate_v31(&conn).unwrap();
    insert_event(&conn, 1, "2026-10-17|munich|zenith").unwrap();
    conn.pragma_update(None, "user_version", 72).unwrap();

    let before: i64 = conn
        .query_row("SELECT COUNT(*) FROM concert_events", [], |row| row.get(0))
        .unwrap();
    crate::db_concerts::migrate_v73(&conn).unwrap();
    let after: i64 = conn
        .query_row("SELECT COUNT(*) FROM concert_events", [], |row| row.get(0))
        .unwrap();
    let availability: String = conn
        .query_row(
            "SELECT ticket_availability FROM concert_events WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(before, after);
    assert_eq!(availability, "unknown");
}

#[test]
fn v75_drops_the_stored_concerts_column_layout_and_keeps_the_widths() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE settings (
           key TEXT PRIMARY KEY,
           value TEXT NOT NULL
         );
         INSERT INTO settings (key, value) VALUES
           ('ui.column_layout.concerts', 'concert-layout'),
           ('ui.column_widths.concerts', 'concert-widths'),
           ('ui.column_layout.releases', 'release-layout');",
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 74).unwrap();

    crate::db_concerts::migrate_v75(&conn).unwrap();
    crate::db_concerts::migrate_v75(&conn).unwrap();

    let settings = conn
        .prepare("SELECT key FROM settings ORDER BY key")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        settings,
        vec![
            "ui.column_layout.releases".to_owned(),
            "ui.column_widths.concerts".to_owned(),
        ]
    );
    assert_eq!(version, 75);
}

#[test]
fn v76_collapses_the_measured_pairs_with_the_runtime_winner_rule() {
    let conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "user_version", 30).unwrap();
    crate::db_concerts::migrate_v31(&conn).unwrap();
    conn.pragma_update(None, "user_version", 72).unwrap();
    crate::db_concerts::migrate_v73(&conn).unwrap();
    conn.pragma_update(None, "user_version", 75).unwrap();
    let cases = [
        (
            "Catch Your Breath",
            "2026-11-15",
            "New Haven",
            ("Toads Place - CT", "https://etix.com/event/other"),
            (
                "Toad's Place",
                "https://ticketmaster.com/event/Z7r9jZ1A70U-U",
            ),
            "https://ticketmaster.com/event/Z7r9jZ1A70U-U",
        ),
        (
            "Chelsea Grin",
            "2026-11-28",
            "Chicago",
            ("Riviera Theatre- IL", "https://axs.com/events/other"),
            (
                "Riviera Theatre",
                "https://ticketmaster.com/event/Z7r9jZ1A7P88F",
            ),
            "https://ticketmaster.com/event/Z7r9jZ1A7P88F",
        ),
        (
            "Electric Callboy",
            "2027-02-14",
            "Amsterdam",
            ("Ziggo Dome", "https://ticketmaster.nl/event/vip-upgrades"),
            (
                "Ziggo Dome Club",
                "https://ticketmaster.nl/event/premium-packages",
            ),
            "https://ticketmaster.nl/event/vip-upgrades",
        ),
        (
            "Ocean Sleeper",
            "2026-09-19",
            "Grand Rapids",
            ("The Intersection", "https://etix.com/event/other"),
            (
                "Intersection",
                "https://ticketmaster.com/event/Z7r9jZ1AAZ3xp",
            ),
            "https://ticketmaster.com/event/Z7r9jZ1AAZ3xp",
        ),
        (
            "Wage War",
            "2027-01-15",
            "Cardiff",
            (
                "Y Plas, Cardiff Students Union",
                "https://universe.com/events/other?ref=ticketmaster",
            ),
            (
                "Cardiff University Students Union",
                "https://ticketmaster.co.uk/event/other",
            ),
            "https://ticketmaster.co.uk/event/other",
        ),
    ];

    for (artist, date, city, first, second, _) in cases {
        for (venue, ticket_url) in [first, second] {
            conn.execute(
                "INSERT INTO concert_events (
                    artist_key, artist_name, starts_at, date_key, venue, city,
                    ticket_url, provider, fetched_at, dedupe_key
                 ) VALUES (?1, ?1, ?2, ?3, ?4, ?5, ?6, 'ticketmaster', 42, ?7)",
                params![
                    artist,
                    format!("{date}T19:00:00"),
                    date,
                    venue,
                    city,
                    ticket_url,
                    format!("{date}|{city}|{venue}")
                ],
            )
            .unwrap();
        }
    }

    crate::db_concerts::migrate_v76(&conn).unwrap();
    crate::db_concerts::migrate_v76(&conn).unwrap();

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM concert_events", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 5);
    for (artist, date, city, _, _, expected_url) in cases {
        let (ticket_url, key): (String, String) = conn
            .query_row(
                "SELECT ticket_url, dedupe_key FROM concert_events WHERE artist_name = ?1",
                [artist],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(ticket_url, expected_url, "{artist}");
        assert_eq!(key, crate::concerts::dedupe_key(artist, date, city));
    }

    let duplicate = insert_event(
        &conn,
        99,
        &crate::concerts::dedupe_key("Catch Your Breath", "2026-11-15", "New Haven"),
    )
    .unwrap_err();
    assert_eq!(
        duplicate.sqlite_error_code(),
        Some(ErrorCode::ConstraintViolation)
    );
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 76);
}

#[test]
fn the_concerts_layout_setting_key_matches_the_frozen_migration_literal() {
    assert_eq!(
        crate::library::settings::CONCERTS_COLUMN_LAYOUT_KEY,
        "ui.column_layout.concerts"
    );
}

#[test]
fn a_v72_database_reaches_v74_with_both_new_columns() {
    let conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "user_version", 30).unwrap();
    crate::db_concerts::migrate_v31(&conn).unwrap();
    conn.execute_batch(
        "CREATE TABLE new_releases (
           release_group_mbid TEXT PRIMARY KEY,
           artist_name TEXT NOT NULL,
           artist_mbid TEXT NOT NULL,
           title TEXT NOT NULL,
           release_type TEXT NOT NULL,
           first_release_date TEXT NOT NULL,
           fetched_at INTEGER NOT NULL,
           seen_at INTEGER,
           hidden INTEGER NOT NULL DEFAULT 0
         );",
    )
    .unwrap();
    conn.pragma_update(None, "user_version", 72).unwrap();

    crate::db_concerts::migrate_v73(&conn).unwrap();
    crate::db_new_releases_notify::migrate_v74(&conn).unwrap();

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    let concert_columns = table_columns(&conn, "concert_events");
    let release_columns = table_columns(&conn, "new_releases");
    assert_eq!(version, 74);
    assert!(concert_columns
        .iter()
        .any(|column| column == "ticket_availability"));
    assert!(release_columns
        .iter()
        .any(|column| column == "notified_released_at"));
}

fn table_columns(conn: &Connection, table: &str) -> Vec<String> {
    let mut statement = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap();
    statement
        .query_map([], |row| row.get(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

#[test]
fn supported_schema_version_is_v76() {
    assert_eq!(crate::db::SUPPORTED_SCHEMA_VERSION, 76);
}
