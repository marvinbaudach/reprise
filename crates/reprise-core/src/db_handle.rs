//! The library database handle every frontend holds.
//!
//! [`Db`] owns the `rusqlite::Connection` and does not hand it out. That is the
//! whole point: a frontend that cannot name a `Connection` cannot grow its own
//! SQL, and cannot wrap the connection in the `Rc<RefCell<_>>` whose borrows
//! are this project's most common panic class.
//!
//! Deliberately **not** implemented: `Deref<Target = Connection>`. It would put
//! the connection back within reach of every caller through the back door and
//! undo the reason this type exists.

use crate::db::{self, DbError};
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};

/// Owns the library database.
///
/// Construct one per process that talks to the library, plus one per worker
/// thread — a `Connection` is not `Sync`, so background work opens its own
/// handle over the same path rather than sharing the frontend's.
pub struct Db {
    conn: Connection,
}

impl Db {
    /// Opens the database at `path` (or an in-memory one for `None`) and
    /// applies every pending schema migration before returning.
    ///
    /// This is the boundary a frontend uses; it must not duplicate
    /// schema-readiness details of its own.
    pub fn open_migrated(path: Option<&Path>) -> Result<Self, DbError> {
        Ok(Self {
            conn: db::open_migrated(path)?,
        })
    }

    /// A migrated, throwaway in-memory database — the fixture constructor for
    /// tests. Equivalent to `open_migrated(None)`, named for what it is at the
    /// call site.
    pub fn open_in_memory() -> Result<Self, DbError> {
        Self::open_migrated(None)
    }

    #[cfg(test)]
    pub(crate) fn from_connection(conn: Connection) -> Self {
        Self { conn }
    }

    /// Opens an existing, already-migrated library without running migrations
    /// or open-time maintenance.
    ///
    /// Stateless readers use this after process startup has prepared the
    /// database. The exact schema check preserves the handle's "ready to use"
    /// invariant while avoiding a maintenance write attempt for every read.
    pub fn open_ready(path: &Path) -> Result<Self, DbError> {
        let conn = db::open(Some(path))?;
        Self::from_ready_connection(conn)
    }

    /// Opens an existing, already-migrated library for background work that
    /// must be physically unable to write.
    ///
    /// Unlike [`Self::open_ready`], this preserves SQLite's read-only open
    /// mode. It is intended for isolated scans that only need to compare
    /// external data with the Reprise library.
    pub fn open_ready_read_only(path: &Path) -> Result<Self, DbError> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        Self::from_ready_connection(conn)
    }

    fn from_ready_connection(conn: Connection) -> Result<Self, DbError> {
        let found = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if found > db::SUPPORTED_SCHEMA_VERSION {
            return Err(DbError::SchemaTooNew {
                found,
                supported: db::SUPPORTED_SCHEMA_VERSION,
            });
        }
        if found < db::SUPPORTED_SCHEMA_VERSION {
            return Err(DbError::SchemaNotReady {
                found,
                supported: db::SUPPORTED_SCHEMA_VERSION,
            });
        }
        Ok(Self { conn })
    }

    /// The file this handle is attached to, or `None` for an in-memory
    /// database.
    ///
    /// Worker threads need this to open their own handle; asking the
    /// connection is more honest than assuming [`db::default_path`], which is
    /// wrong under a test fixture or an explicitly chosen library.
    pub fn path(&self) -> Option<PathBuf> {
        db::main_path_connection(&self.conn)
    }

    /// The underlying connection.
    ///
    /// Core's private SQL boundary. Frontends and adapters receive the handle
    /// itself and go through named Core facades.
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }
}

impl std::fmt::Debug for Db {
    /// Hand-written because `Connection`'s own `Debug` is opaque anyway; the
    /// path is the part worth seeing in a log line or a failing assertion.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Db")
            .field("path", &self.path())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[path = "db_handle_tests.rs"]
mod tests;
