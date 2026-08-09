//! Persisted clocks for the Missing files and Import errors views.

use rusqlite::Connection;

use super::{get_setting_in, set_setting_in};

/// Unix-seconds timestamp of the last time the user opened the Missing
/// files / Import errors views — the clock the sidebar's ISSUES badges
/// (Task 2.5) are keyed on. `queries::count_new_missing`/`queries::count_
/// new_import_errors` take this as a plain `i64` parameter rather than
/// reading it internally, the same "boundary arithmetic stays testable"
/// shape `auto_clean_eligible`'s `now: i64` parameter uses — a pure function
/// over an explicit timestamp needs no fake clock to test its `>` boundary.
///
/// A never-written key reads back as `0`, not an error and not `None`:
/// `0` is deliberately BELOW any real `first_seen`/`missing_since` unix
/// timestamp this app will ever record, so "never viewed" naturally makes
/// every existing issue count as new — exactly the behavior a first-run
/// user should see (there IS a backlog, and they haven't looked at it yet).
/// A stored value that fails to parse as `i64` (hand-edited/corrupt
/// database) degrades to this same `0` fallback, never an error — the same
/// "fail toward showing more, not toward crashing" posture
/// `AutoCleanSetting::parse` argues for on the opposite (destructive) side
/// of this codebase's settings.
pub const LAST_VIEWED_MISSING_KEY: &str = "last_viewed_missing";
pub const LAST_VIEWED_IMPORT_ERRORS_KEY: &str = "last_viewed_import_errors";

/// See [`LAST_VIEWED_MISSING_KEY`]'s doc comment for the "missing key or
/// corrupt value both read back as 0" contract this and its three siblings
/// below share.
pub(super) fn get_last_viewed_missing_in(conn: &Connection) -> Result<i64, rusqlite::Error> {
    let stored = get_setting_in(conn, LAST_VIEWED_MISSING_KEY)?;
    Ok(stored
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0))
}

/// Writes `now` as the Missing-files view's last-viewed timestamp — the
/// view calls this the moment it opens, which is what clears
/// `queries::count_new_missing`'s badge for everything currently visible.
pub(super) fn set_last_viewed_missing_in(
    conn: &Connection,
    now: i64,
) -> Result<(), rusqlite::Error> {
    set_setting_in(conn, LAST_VIEWED_MISSING_KEY, &now.to_string())
}

pub(super) fn get_last_viewed_import_errors_in(conn: &Connection) -> Result<i64, rusqlite::Error> {
    let stored = get_setting_in(conn, LAST_VIEWED_IMPORT_ERRORS_KEY)?;
    Ok(stored
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0))
}

pub(super) fn set_last_viewed_import_errors_in(
    conn: &Connection,
    now: i64,
) -> Result<(), rusqlite::Error> {
    set_setting_in(conn, LAST_VIEWED_IMPORT_ERRORS_KEY, &now.to_string())
}
