//! One-time startup: open and migrate the database — applying the fail-closed
//! [`SchemaTooNew`](reprise_core::db::DbError::SchemaTooNew) guard (Beschluss 8)
//! — and snapshot the `playlist:create` capability before the server serves.

use std::path::Path;

use reprise_core::db::{self, DbError};

use crate::capability;

/// A startup failure that prevents the server from serving.
#[derive(Debug)]
pub enum StartupError {
    /// The on-disk schema is newer than this build supports (fail-closed):
    /// the server refuses to start rather than run against a schema it cannot
    /// understand.
    SchemaTooNew { found: i64, supported: i64 },
    /// The database could not be opened or migrated.
    Open(DbError),
    /// The capability snapshot query failed.
    Query(rusqlite::Error),
}

/// Opens and migrates the database, then returns whether `playlist:create`
/// was granted at startup (the restart-gated half of the D18 write gate).
pub fn prepare(db_path: &Path) -> Result<bool, StartupError> {
    let conn = match db::open_migrated(Some(db_path)) {
        Ok(conn) => conn,
        Err(DbError::SchemaTooNew { found, supported }) => {
            return Err(StartupError::SchemaTooNew { found, supported });
        }
        Err(other) => return Err(StartupError::Open(other)),
    };
    capability::playlist_create_granted(&conn).map_err(StartupError::Query)
}
