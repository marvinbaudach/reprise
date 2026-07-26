//! One-time startup: open and migrate the database — applying the fail-closed
//! [`SchemaTooNew`](reprise_core::db::DbError::SchemaTooNew) guard (Beschluss 8)
//! — and snapshot the write-class capabilities before the server serves.

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

/// The write-class capability snapshot taken at startup — the restart-gated
/// half of the D18 / Beschluss 7 gate (a fresh grant only takes effect after a
/// restart; a revocation is caught live on every call).
#[derive(Debug, Clone, Copy)]
pub struct StartupCaps {
    /// Whether `playlist:create` was granted at startup.
    pub playlist_create: bool,
    /// Whether `playlist:manage` was granted at startup.
    pub playlist_manage: bool,
    /// Whether `ai:create` was granted at startup.
    pub ai_create: bool,
    /// Whether `sources:manage` was granted at startup.
    pub sources_manage: bool,
    #[cfg(feature = "mpris")]
    pub device_sync: bool,
}

/// Opens and migrates the database, then snapshots the write-class capabilities
/// (`playlist:create`, `playlist:manage`, `ai:create`, `sources:manage`) as they
/// stood at startup.
pub fn prepare(db_path: &Path) -> Result<StartupCaps, StartupError> {
    let conn = match db::open_migrated(Some(db_path)) {
        Ok(conn) => conn,
        Err(DbError::SchemaTooNew { found, supported }) => {
            return Err(StartupError::SchemaTooNew { found, supported });
        }
        Err(other) => return Err(StartupError::Open(other)),
    };
    Ok(StartupCaps {
        playlist_create: capability::playlist_create_granted(&conn).map_err(StartupError::Query)?,
        playlist_manage: capability::playlist_manage_granted(&conn).map_err(StartupError::Query)?,
        ai_create: capability::ai_create_granted(&conn).map_err(StartupError::Query)?,
        sources_manage: capability::sources_manage_granted(&conn).map_err(StartupError::Query)?,
        #[cfg(feature = "mpris")]
        device_sync: capability::device_sync_granted(&conn).map_err(StartupError::Query)?,
    })
}
