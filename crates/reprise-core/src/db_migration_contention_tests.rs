//! Two processes opening the same un-migrated database at once (the app and
//! the CLI after an upgrade). A step reads `user_version` and its own
//! preconditions in autocommit, so a rival that finishes the step first makes
//! those reads stale; a deferred transaction then re-applies the step (a
//! second `CREATE TABLE`, `ADD COLUMN`) or fails the write-lock upgrade with
//! `SQLITE_BUSY_SNAPSHOT` (extended code 517), which `busy_timeout` never
//! retries (#1188). The rival is placed deterministically: SQLite consults
//! the authorizer while the writer *prepares* its `BEGIN`, which is after the
//! step's reads and before it holds any lock.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use rusqlite::hooks::{AuthAction, AuthContext, Authorization, TransactionOperation};
use rusqlite::Connection;

use crate::db::SUPPORTED_SCHEMA_VERSION;
use crate::library::rival_commit_test_support::arm_on_first_write;

const DB_FILE: &str = "reprise.db";
const MID_CHAIN_VERSION: i64 = 68;

struct Scene {
    directory: tempfile::TempDir,
    caches: tempfile::TempDir,
}

impl Scene {
    fn new() -> Self {
        Self {
            directory: tempfile::tempdir().unwrap(),
            caches: tempfile::tempdir().unwrap(),
        }
    }

    fn path(&self) -> PathBuf {
        self.directory.path().join(DB_FILE)
    }

    fn open(&self) -> Connection {
        crate::db::open(Some(&self.path())).unwrap()
    }

    fn migrate(&self, conn: &Connection) -> Result<(), crate::db::DbError> {
        crate::db::migrate_with_cache_dirs(conn, self.caches.path(), self.caches.path())
    }

    /// Makes the first `BEGIN` `writer` prepares let a rival connection run
    /// the whole migration first. The rival runs on its own thread, as a
    /// second process would: a step's noted start is per thread.
    fn rival_migrates_at_first_begin(&self, writer: &Connection) -> Arc<AtomicBool> {
        let path = self.path();
        let caches: PathBuf = self.caches.path().to_path_buf();
        let fired = Arc::new(AtomicBool::new(false));
        let hook_fired = Arc::clone(&fired);
        writer
            .authorizer(Some(move |context: AuthContext<'_>| {
                let begins = matches!(
                    context.action,
                    AuthAction::Transaction {
                        operation: TransactionOperation::Begin
                    }
                );
                if begins && !hook_fired.swap(true, Ordering::SeqCst) {
                    let (path, caches) = (path.clone(), caches.clone());
                    std::thread::spawn(move || {
                        let rival = crate::db::open(Some(&path)).unwrap();
                        rival.pragma_update(None, "busy_timeout", 0).unwrap();
                        let _ = crate::db::migrate_with_cache_dirs(&rival, &caches, &caches);
                    })
                    .join()
                    .unwrap();
                }
                Authorization::Allow
            }))
            .unwrap();
        fired
    }
}

fn user_version(conn: &Connection) -> i64 {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn a_fresh_database_survives_a_rival_that_migrates_it_first() {
    let scene = Scene::new();
    let writer = scene.open();
    let fired = scene.rival_migrates_at_first_begin(&writer);

    scene.migrate(&writer).unwrap();

    assert!(fired.load(Ordering::SeqCst), "the rival must have migrated");
    assert_eq!(user_version(&writer), SUPPORTED_SCHEMA_VERSION);
}

#[test]
fn a_half_migrated_database_survives_a_rival_that_finishes_it_first() {
    let scene = Scene::new();
    let writer = scene.open();
    crate::db_schema_baseline::migrate_baseline(
        &writer,
        false,
        scene.caches.path(),
        scene.caches.path(),
    )
    .unwrap();
    crate::db_migrations::run_migrations_through(&writer, MID_CHAIN_VERSION).unwrap();
    let fired = scene.rival_migrates_at_first_begin(&writer);

    scene.migrate(&writer).unwrap();

    assert!(fired.load(Ordering::SeqCst), "the rival must have migrated");
    assert_eq!(user_version(&writer), SUPPORTED_SCHEMA_VERSION);
}

#[test]
fn a_step_that_reads_before_it_writes_survives_a_rival_commit_between_the_two() {
    let scene = Scene::new();
    let writer = scene.open();
    scene.migrate(&writer).unwrap();
    writer.pragma_update(None, "user_version", 75).unwrap();
    writer
        .execute(
            "INSERT INTO concert_events (
                artist_key, artist_name, starts_at, date_key, venue, city,
                provider, fetched_at, dedupe_key
             ) VALUES ('artist', 'Artist', '2026-10-17T19:00:00', '2026-10-17',
                       'Hall', 'Munich', 'bandsintown', 42, 'legacy-key')",
            [],
        )
        .unwrap();
    let fired = arm_on_first_write(&writer, &scene.path(), "concert_events");

    crate::db_concerts::migrate_v76(&writer).unwrap();

    assert!(
        fired.load(Ordering::SeqCst),
        "the rival must have committed"
    );
    assert_eq!(user_version(&writer), 76);
}

#[test]
fn opening_a_current_database_never_waits_for_the_write_lock() {
    let scene = Scene::new();
    let writer = scene.open();
    scene.migrate(&writer).unwrap();
    let holder = scene.open();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    let opener = scene.open();
    opener.pragma_update(None, "busy_timeout", 0).unwrap();

    scene.migrate(&opener).unwrap();

    assert_eq!(user_version(&opener), SUPPORTED_SCHEMA_VERSION);
}

/// A rival inside a long step (the v90 `tracks` rebuild) keeps the write lock
/// past `busy_timeout`. `BEGIN IMMEDIATE` that is refused holds nothing, so the
/// step is simply started again; the holder lets go when it is asked a second
/// time.
#[test]
fn a_step_that_finds_the_write_lock_held_is_started_again() {
    let scene = Scene::new();
    let opener = scene.open();
    opener.pragma_update(None, "busy_timeout", 0).unwrap();
    let holder = scene.open();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    let begins = Arc::new(AtomicUsize::new(0));
    let hook_begins = Arc::clone(&begins);
    opener
        .authorizer(Some(move |context: AuthContext<'_>| {
            let begins = matches!(
                context.action,
                AuthAction::Transaction {
                    operation: TransactionOperation::Begin
                }
            );
            if begins && hook_begins.fetch_add(1, Ordering::SeqCst) == 1 {
                holder.execute_batch("ROLLBACK").unwrap();
            }
            Authorization::Allow
        }))
        .unwrap();

    scene.migrate(&opener).unwrap();

    assert!(
        begins.load(Ordering::SeqCst) >= 2,
        "the step must have been retried"
    );
    assert_eq!(user_version(&opener), SUPPORTED_SCHEMA_VERSION);
}

/// Whoever notes a step's start and then never opens its transaction (the
/// baseline steps end at their own version) must not leave a basis behind for
/// a later, unrelated `begin_step` on the same thread to trip over.
#[test]
fn a_noted_start_does_not_outlive_the_step_that_noted_it() {
    let scene = Scene::new();
    let conn = scene.open();
    crate::db_schema_baseline::migrate_baseline(
        &conn,
        false,
        scene.caches.path(),
        scene.caches.path(),
    )
    .unwrap();
    let rival = scene.open();
    rival
        .execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('rival', '1')",
            [],
        )
        .unwrap();

    let transaction = crate::db_migrations::begin_step(&conn);

    assert!(transaction.is_ok(), "{:?}", transaction.err());
}
