//! Tiny key/value settings store (Stage 3 Task 8 — schema v4's `settings`
//! table, see `db.rs`'s `SCHEMA_V4` doc comment). The one consumer this task
//! adds is `library_root` (`LIBRARY_ROOT_KEY`): the folder the user last
//! scanned, persisted here so the folder watcher (`library::watcher`) knows
//! what to watch on startup without the user re-picking a folder every
//! launch. Deliberately generic (`get_setting`/`set_setting` take any `&str`
//! key) rather than one bespoke function per setting — a future setting is
//! then just one more constant and call site, not a new migration.

use rusqlite::{Connection, OptionalExtension};

#[path = "settings_api.rs"]
mod api;
pub use api::*;
#[path = "settings_column_keys.rs"]
mod column_keys;
pub use column_keys::*;
#[path = "settings_geometry.rs"]
mod geometry;
pub use geometry::*;
#[path = "settings_layout.rs"]
mod layout;
pub use layout::*;
#[path = "settings_playback.rs"]
mod playback;
pub use playback::*;
#[path = "settings_auto_clean.rs"]
mod auto_clean;
pub use auto_clean::*;
/// The settings key `ui::window`'s scan flow writes the scanned folder under,
/// and `main.rs`/`ui::window` read at startup/after-scan to (re)start the
/// watcher. `pub` so both call sites share the exact same literal rather than
/// risking a typo'd duplicate string.
pub const LIBRARY_ROOT_KEY: &str = "library_root";
pub const ONBOARDING_COMPLETED_KEY: &str = "onboarding.completed";
pub const ONLINE_SOURCES_FIRST_ENABLE_COMPLETED_KEY: &str = "online_sources.first_enable_completed";
pub const ARTWORK_CONSENT_MERGE_NOTICE_PENDING_KEY: &str = "artwork.consent_merge_notice_pending";
pub const NEW_RELEASES_FETCH_COMPLETED_KEY: &str = "new_releases.fetch_completed";
pub const NEW_RELEASES_LAST_COMPLETED_AT_KEY: &str = "new_releases.last_completed_at";
pub const LAST_SCAN_RELINKED_KEY: &str = "last_scan_relinked";

/// Reads `key`'s current value, if any has ever been set. `Ok(None)` — not
/// an error — for a key that has never been written, matching every other
/// "not found" case in this codebase's query layer (e.g. `queries::query_
/// track_summary`).
pub(crate) fn get_setting_in(
    conn: &Connection,
    key: &str,
) -> Result<Option<String>, rusqlite::Error> {
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
pub(crate) fn set_setting_in(
    conn: &Connection,
    key: &str,
    value: &str,
) -> Result<(), rusqlite::Error> {
    // Every settings write funnels through here, so a single change-log append
    // covers both plain settings and module toggles (which persist under the
    // `module.<id>.enabled` key via `modules::set_enabled`) — exactly one event
    // per write, keyed by the setting key. `in_txn` keeps the row and the event
    // atomic without a nested `BEGIN` when a caller already holds one.
    crate::events::in_txn(conn, |conn| {
        // Dedup (mirrors `create_smart`): an identical stored value is a
        // genuine no-op, so it must neither rewrite the row nor append a
        // `change_log` event — otherwise every idempotent settings write (e.g.
        // re-persisting an unchanged layout) would wake every other frontend
        // for nothing. Reading inside the same transaction keeps the
        // check-then-write atomic against a concurrent writer.
        let current: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                rusqlite::params![key],
                |r| r.get(0),
            )
            .optional()?;
        if current.as_deref() == Some(value) {
            return Ok(());
        }
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET value = ?2",
            rusqlite::params![key, value],
        )?;
        crate::events::record(conn, "settings", key, "set")?;
        Ok(())
    })
}

/// Canonical stored forms for boolean settings. `get_bool` additionally
/// tolerates anything else by falling back to the caller's default (never
/// crash on a hand-edited database; log and move on — the same tolerance
/// posture as the scanner's).
const BOOL_TRUE: &str = "1";
const BOOL_FALSE: &str = "0";

pub(crate) fn get_bool_in(
    conn: &Connection,
    key: &str,
    default: bool,
) -> Result<bool, rusqlite::Error> {
    match get_setting_in(conn, key)? {
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

pub(crate) fn set_bool_in(
    conn: &Connection,
    key: &str,
    value: bool,
) -> Result<(), rusqlite::Error> {
    set_setting_in(conn, key, if value { BOOL_TRUE } else { BOOL_FALSE })
}

/// Typed accessors for `LIBRARY_ROOT_KEY` — the one string setting with
/// scattered call sites today (main.rs dev hook, scan flow, watcher
/// startup). Stored as the same string the scanner writes; kept as String
/// (not PathBuf) because the scanner's path storage is string-based and a
/// lossy round-trip here could diverge from what `mark_vanished_under_root`
/// compares against.
fn get_library_root_in(conn: &Connection) -> Result<Option<String>, rusqlite::Error> {
    get_setting_in(conn, LIBRARY_ROOT_KEY)
}

fn set_library_root_in(conn: &Connection, root: &str) -> Result<(), rusqlite::Error> {
    set_setting_in(conn, LIBRARY_ROOT_KEY, root)
}

fn get_last_scan_relinked_in(conn: &Connection) -> Result<Option<u32>, rusqlite::Error> {
    Ok(get_setting_in(conn, LAST_SCAN_RELINKED_KEY)?.and_then(|value| value.parse::<u32>().ok()))
}

pub(super) fn set_last_scan_relinked_in(
    conn: &Connection,
    count: u32,
) -> Result<(), rusqlite::Error> {
    set_setting_in(conn, LAST_SCAN_RELINKED_KEY, &count.to_string())
}

fn get_onboarding_completed_in(conn: &Connection) -> Result<bool, rusqlite::Error> {
    get_bool_in(conn, ONBOARDING_COMPLETED_KEY, false)
}

fn set_onboarding_completed_in(conn: &Connection, completed: bool) -> Result<(), rusqlite::Error> {
    set_bool_in(conn, ONBOARDING_COMPLETED_KEY, completed)
}

fn get_new_releases_fetch_completed_in(conn: &Connection) -> Result<bool, rusqlite::Error> {
    get_bool_in(conn, NEW_RELEASES_FETCH_COMPLETED_KEY, false)
}

fn set_new_releases_fetch_completed_in(
    conn: &Connection,
    completed: bool,
) -> Result<(), rusqlite::Error> {
    set_bool_in(conn, NEW_RELEASES_FETCH_COMPLETED_KEY, completed)
}

fn get_new_releases_last_completed_at_in(
    conn: &Connection,
) -> Result<Option<i64>, rusqlite::Error> {
    Ok(get_setting_in(conn, NEW_RELEASES_LAST_COMPLETED_AT_KEY)?
        .and_then(|value| value.parse::<i64>().ok()))
}

fn set_new_releases_last_completed_at_in(
    conn: &Connection,
    completed_at: i64,
) -> Result<(), rusqlite::Error> {
    set_setting_in(
        conn,
        NEW_RELEASES_LAST_COMPLETED_AT_KEY,
        &completed_at.to_string(),
    )
}

fn typed_value(conn: &Connection, key: &str, default: &'static str) -> String {
    match get_setting_in(conn, key) {
        Ok(Some(value)) => value,
        Ok(None) => default.to_string(),
        Err(error) => {
            tracing::warn!(%error, key, "could not read typed setting; using default");
            default.to_string()
        }
    }
}

#[path = "settings_issue_views.rs"]
mod issue_views;
use issue_views::{
    get_last_viewed_import_errors_in, get_last_viewed_missing_in, set_last_viewed_import_errors_in,
    set_last_viewed_missing_in,
};
pub use issue_views::{LAST_VIEWED_IMPORT_ERRORS_KEY, LAST_VIEWED_MISSING_KEY};

#[path = "settings_seek.rs"]
mod seek;
pub use seek::*;
#[cfg(test)]
#[path = "settings_compact_tests.rs"]
mod compact_tests;

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
