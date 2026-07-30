//! File-backed database fixtures for GNOME tests that need direct SQL seeding.
//!
//! Production code only receives [`Db`]. Tests may open a separate, short-lived
//! connection to the same throwaway file so fixture setup does not widen
//! Core's public database boundary.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use reprise_core::db::{Db, DbError};
use rusqlite::Connection;

static FIXTURE_ROOT: OnceLock<tempfile::TempDir> = OnceLock::new();
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

pub(crate) fn open() -> Result<Db, DbError> {
    let root = FIXTURE_ROOT.get_or_init(|| {
        tempfile::Builder::new()
            .prefix("reprise-gnome-tests-")
            .tempdir()
            .expect("create GNOME database fixture directory")
    });
    let id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
    let path = root.path().join(format!("fixture-{id}.sqlite3"));
    Db::open_migrated(Some(&path))
}

pub(crate) fn connection(db: &Db) -> Connection {
    let path = db.path().expect("GNOME test database must be file-backed");
    let connection = Connection::open(path).expect("open GNOME database fixture connection");
    connection
        .busy_timeout(Duration::from_secs(5))
        .expect("configure GNOME database fixture busy timeout");
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .expect("enable foreign keys for GNOME database fixture");
    connection
}
