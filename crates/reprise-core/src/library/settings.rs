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

/// Canonical stored forms for boolean settings. `get_bool` additionally
/// tolerates anything else by falling back to the caller's default (never
/// crash on a hand-edited database; log and move on — the same tolerance
/// posture as the scanner's).
const BOOL_TRUE: &str = "1";
const BOOL_FALSE: &str = "0";

pub fn get_bool(conn: &Connection, key: &str, default: bool) -> Result<bool, rusqlite::Error> {
    match get_setting(conn, key)? {
        None => Ok(default),
        Some(value) => match value.as_str() {
            BOOL_TRUE => Ok(true),
            BOOL_FALSE => Ok(false),
            other => {
                tracing::warn!(
                    key,
                    value = other,
                    "unrecognized boolean setting; using default"
                );
                Ok(default)
            }
        },
    }
}

pub fn set_bool(conn: &Connection, key: &str, value: bool) -> Result<(), rusqlite::Error> {
    set_setting(conn, key, if value { BOOL_TRUE } else { BOOL_FALSE })
}

/// Typed accessors for `LIBRARY_ROOT_KEY` — the one string setting with
/// scattered call sites today (main.rs dev hook, scan flow, watcher
/// startup). Stored as the same string the scanner writes; kept as String
/// (not PathBuf) because the scanner's path storage is string-based and a
/// lossy round-trip here could diverge from what `mark_vanished_under_root`
/// compares against.
pub fn get_library_root(conn: &Connection) -> Result<Option<String>, rusqlite::Error> {
    get_setting(conn, LIBRARY_ROOT_KEY)
}

pub fn set_library_root(conn: &Connection, root: &str) -> Result<(), rusqlite::Error> {
    set_setting(conn, LIBRARY_ROOT_KEY, root)
}

pub const PLAYER_BAR_POSITION_KEY: &str = "player_bar_position";
pub const COLUMN_LAYOUT_KEY: &str = "ui.column_layout";

/// Where the player bar docks. `Bottom` is the default and the fallback for any
/// unknown/hand-edited value (same tolerance posture as `get_bool`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerBarPosition {
    Top,
    Bottom,
}

pub fn get_player_bar_position(conn: &Connection) -> PlayerBarPosition {
    match get_setting(conn, PLAYER_BAR_POSITION_KEY) {
        Ok(Some(v)) if v == "top" => PlayerBarPosition::Top,
        Ok(Some(v)) if v == "bottom" => PlayerBarPosition::Bottom,
        Ok(Some(other)) => {
            tracing::warn!(value = %other, "unrecognized player_bar_position; using Bottom");
            PlayerBarPosition::Bottom
        }
        Ok(None) => PlayerBarPosition::Bottom,
        Err(error) => {
            tracing::warn!(%error, "could not read player_bar_position; using Bottom");
            PlayerBarPosition::Bottom
        }
    }
}

pub fn set_player_bar_position(
    conn: &Connection,
    pos: PlayerBarPosition,
) -> Result<(), rusqlite::Error> {
    let value = match pos {
        PlayerBarPosition::Top => "top",
        PlayerBarPosition::Bottom => "bottom",
    };
    set_setting(conn, PLAYER_BAR_POSITION_KEY, value)
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

    #[test]
    fn get_bool_returns_default_when_never_set() {
        let conn = migrated_conn();
        assert!(get_bool(&conn, "module.mpris.enabled", true).unwrap());
        assert!(!get_bool(&conn, "module.mpris.enabled", false).unwrap());
    }

    #[test]
    fn set_bool_round_trips_both_values() {
        let conn = migrated_conn();
        set_bool(&conn, "flag", true).unwrap();
        assert!(get_bool(&conn, "flag", false).unwrap());
        set_bool(&conn, "flag", false).unwrap();
        assert!(!get_bool(&conn, "flag", true).unwrap());
    }

    #[test]
    fn get_bool_falls_back_to_default_on_unrecognized_value() {
        // A hand-edited or future-version value must never crash or silently
        // flip a feature: unrecognized -> default, with a warning logged.
        let conn = migrated_conn();
        set_setting(&conn, "flag", "banana").unwrap();
        assert!(get_bool(&conn, "flag", true).unwrap());
        assert!(!get_bool(&conn, "flag", false).unwrap());
    }

    #[test]
    fn library_root_typed_accessors_round_trip() {
        let conn = migrated_conn();
        assert_eq!(get_library_root(&conn).unwrap(), None);
        set_library_root(&conn, "/music/library").unwrap();
        assert_eq!(
            get_library_root(&conn).unwrap(),
            Some("/music/library".to_string())
        );
    }

    #[test]
    fn player_bar_position_defaults_to_bottom() {
        let conn = migrated_conn();
        assert_eq!(get_player_bar_position(&conn), PlayerBarPosition::Bottom);
    }

    #[test]
    fn player_bar_position_round_trips_both_values() {
        let conn = migrated_conn();
        set_player_bar_position(&conn, PlayerBarPosition::Top).unwrap();
        assert_eq!(get_player_bar_position(&conn), PlayerBarPosition::Top);
        set_player_bar_position(&conn, PlayerBarPosition::Bottom).unwrap();
        assert_eq!(get_player_bar_position(&conn), PlayerBarPosition::Bottom);
    }

    #[test]
    fn player_bar_position_falls_back_to_bottom_on_unknown_value() {
        let conn = migrated_conn();
        set_setting(&conn, PLAYER_BAR_POSITION_KEY, "sideways").unwrap();
        assert_eq!(get_player_bar_position(&conn), PlayerBarPosition::Bottom);
    }
}
