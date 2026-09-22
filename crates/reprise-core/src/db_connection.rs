use std::path::{Path, PathBuf};

use rusqlite::Connection;

use super::{DbError, DEFAULT_BUSY_TIMEOUT_MS};

pub(crate) fn open(path: Option<&Path>) -> Result<Connection, DbError> {
    open_with_options(path, DEFAULT_BUSY_TIMEOUT_MS)
}

/// Opens a connection like [`open`] but with an explicit `busy_timeout` in
/// milliseconds. [`DEFAULT_BUSY_TIMEOUT_MS`] is the value every existing call
/// site keeps (that is exactly what [`open`] passes); a value of `0` makes lock
/// contention fail immediately with `SQLITE_BUSY` rather than block — the
/// non-blocking posture `open_migrated`'s prune uses so a fresh open never
/// stalls behind a long foreign write transaction.
pub(crate) fn open_with_options(
    path: Option<&Path>,
    busy_timeout_ms: i64,
) -> Result<Connection, DbError> {
    let conn = match path {
        Some(path) => {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            Connection::open(path)?
        }
        None => Connection::open_in_memory()?,
    };
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "busy_timeout", busy_timeout_ms)?;
    Ok(conn)
}

/// The file this connection is attached to, if it has one.
///
/// Background work opens its own connection rather than sharing the
/// frontend's, so it needs the path — and asking the connection is more
/// honest than assuming `default_path`, which is wrong under a test fixture or
/// an explicitly chosen library. An in-memory database has no file and yields
/// `None`.
pub(crate) fn main_path_connection(conn: &Connection) -> Option<PathBuf> {
    let mut statement = conn.prepare("PRAGMA database_list").ok()?;
    let mut rows = statement.query([]).ok()?;
    while let Some(row) = rows.next().ok()? {
        let name = row.get::<_, String>(1).ok()?;
        let path = row.get::<_, String>(2).ok()?;
        if name == "main" && !path.is_empty() {
            return Some(PathBuf::from(path));
        }
    }
    None
}
