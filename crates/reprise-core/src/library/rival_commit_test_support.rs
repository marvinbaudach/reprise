//! Deterministic "a rival connection commits mid-transaction" fixture.
//!
//! A read followed by a write inside a DEFERRED transaction fails at once with
//! `SQLITE_BUSY_SNAPSHOT` (extended code 517) when another connection commits
//! between the two — an error `busy_timeout` never retries (#1173). Racing two
//! threads proves nothing reliably, so the rival commit is placed by an
//! authorizer: SQLite consults it while the writer *prepares* its first insert
//! into the watched table, which is exactly after the transaction's read and
//! before its write.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use rusqlite::Connection;

/// Arms `writer` so the first insert it prepares into `table` makes a rival
/// connection to the database at `path` attempt a write. The rival may win
/// (the writer's transaction was deferred) or lose to the held write lock (it
/// was immediate); only the writer under test must always succeed. Returns the
/// flag the authorizer raises once the rival attempt has been made, so a test
/// can prove the interleaving really happened.
pub(crate) fn arm(writer: &Connection, path: &Path, table: &'static str) -> Arc<AtomicBool> {
    let rival = Connection::open(path).unwrap();
    rival.pragma_update(None, "busy_timeout", 0).unwrap();
    let interleaved = Arc::new(AtomicBool::new(false));
    let hook_interleaved = Arc::clone(&interleaved);
    writer
        .authorizer(Some(move |context: AuthContext<'_>| {
            let inserting = matches!(
                context.action,
                AuthAction::Insert { table_name } if table_name == table
            );
            if inserting && !hook_interleaved.swap(true, Ordering::SeqCst) {
                let _ = rival.execute(
                    "INSERT OR REPLACE INTO settings (key, value) VALUES ('rival', '1')",
                    [],
                );
            }
            Authorization::Allow
        }))
        .unwrap();
    interleaved
}
