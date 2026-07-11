//! Tiny key/value settings store (Stage 3 Task 8 — schema v4's `settings`
//! table, see `db.rs`'s `SCHEMA_V4` doc comment). The one consumer this task
//! adds is `library_root` (`LIBRARY_ROOT_KEY`): the folder the user last
//! scanned, persisted here so the folder watcher (`library::watcher`) knows
//! what to watch on startup without the user re-picking a folder every
//! launch. Deliberately generic (`get_setting`/`set_setting` take any `&str`
//! key) rather than one bespoke function per setting — a future setting is
//! then just one more constant and call site, not a new migration.

use rusqlite::{Connection, OptionalExtension};

/// The settings key `ui::window`'s scan flow writes the scanned folder under,
/// and `main.rs`/`ui::window` read at startup/after-scan to (re)start the
/// watcher. `pub` so both call sites share the exact same literal rather than
/// risking a typo'd duplicate string.
pub const LIBRARY_ROOT_KEY: &str = "library_root";

/// Reads `key`'s current value, if any has ever been set. `Ok(None)` — not
/// an error — for a key that has never been written, matching every other
/// "not found" case in this codebase's query layer (e.g. `queries::query_
/// track_summary`).
pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>, rusqlite::Error> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        rusqlite::params![key],
        |r| r.get(0),
    )
    .optional()
}

/// Writes `key` = `value`, overwriting any previous value — an upsert via
/// `ON CONFLICT`, not a delete-then-insert (keeps this a single statement,
/// no transaction needed).
pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = ?2",
        rusqlite::params![key, value],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn migrated_conn() -> Connection {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn get_setting_returns_none_when_never_set() {
        let conn = migrated_conn();
        assert_eq!(get_setting(&conn, "nope").unwrap(), None);
    }

    #[test]
    fn set_then_get_round_trips() {
        let conn = migrated_conn();
        set_setting(&conn, LIBRARY_ROOT_KEY, "/music/library").unwrap();
        assert_eq!(
            get_setting(&conn, LIBRARY_ROOT_KEY).unwrap(),
            Some("/music/library".to_string())
        );
    }

    #[test]
    fn set_setting_overwrites_a_previous_value() {
        let conn = migrated_conn();
        set_setting(&conn, LIBRARY_ROOT_KEY, "/first").unwrap();
        set_setting(&conn, LIBRARY_ROOT_KEY, "/second").unwrap();
        assert_eq!(
            get_setting(&conn, LIBRARY_ROOT_KEY).unwrap(),
            Some("/second".to_string())
        );
        // Exactly one row for this key — the upsert never leaves a stale
        // duplicate behind.
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM settings WHERE key = ?1",
                rusqlite::params![LIBRARY_ROOT_KEY],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn different_keys_do_not_clobber_each_other() {
        let conn = migrated_conn();
        set_setting(&conn, "a", "1").unwrap();
        set_setting(&conn, "b", "2").unwrap();
        assert_eq!(get_setting(&conn, "a").unwrap(), Some("1".to_string()));
        assert_eq!(get_setting(&conn, "b").unwrap(), Some("2".to_string()));
    }
}
