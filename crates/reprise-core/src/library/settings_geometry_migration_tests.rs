//! Schema v80: one-time cleanup of self-certified list geometry.

use rusqlite::{Connection, OptionalExtension};

fn open_v79_database() -> Connection {
    let conn = super::open(None).unwrap();
    super::migrate_connection(&conn).unwrap();
    conn.pragma_update(None, "user_version", 79).unwrap();
    conn
}

fn set(conn: &Connection, key: &str, value: &str) {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)",
        rusqlite::params![key, value],
    )
    .unwrap();
}

fn get(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
        row.get(0)
    })
    .optional()
    .unwrap()
}

fn all_settings(conn: &Connection) -> Vec<(String, String)> {
    conn.prepare("SELECT key, value FROM settings ORDER BY key")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

#[test]
fn v80_clears_persisted_list_geometry_on_the_way_to_the_current_version() {
    let conn = open_v79_database();
    set(&conn, "ui.row_height", "30");
    set(&conn, "ui.section_header_height", "36");

    super::migrate_connection(&conn).unwrap();

    assert_eq!(get(&conn, "ui.row_height"), None);
    assert_eq!(get(&conn, "ui.section_header_height"), None);
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, super::SUPPORTED_SCHEMA_VERSION);
}

#[test]
fn v80_is_a_no_op_when_run_a_second_time() {
    let conn = open_v79_database();
    set(&conn, "ui.row_height", "30");

    crate::library::settings::migrate_v80(&conn).unwrap();
    set(&conn, "ui.row_height", "45");
    let before_second_run = all_settings(&conn);

    crate::library::settings::migrate_v80(&conn).unwrap();

    assert_eq!(all_settings(&conn), before_second_run);
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 80);
}

#[test]
fn v80_preserves_unrelated_settings() {
    let conn = open_v79_database();
    set(&conn, "ui.row_height", "30");
    set(&conn, "ui.column_widths", r#"{"title":320,"artist":180}"#);

    super::migrate_connection(&conn).unwrap();

    assert_eq!(get(&conn, "ui.row_height"), None);
    assert_eq!(
        get(&conn, "ui.column_widths").as_deref(),
        Some(r#"{"title":320,"artist":180}"#)
    );
}

#[test]
fn v80_leaves_an_already_current_database_alone() {
    let conn = open_v79_database();
    set(&conn, "ui.row_height", "45");
    set(&conn, "ui.section_header_height", "49");
    conn.pragma_update(None, "user_version", 80).unwrap();
    let before = all_settings(&conn);

    crate::library::settings::migrate_v80(&conn).unwrap();

    assert_eq!(all_settings(&conn), before);
}
