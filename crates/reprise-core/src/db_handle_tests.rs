use super::*;
use crate::db::SUPPORTED_SCHEMA_VERSION;

/// The handle's whole promise to a frontend is "this is ready to use" — an
/// unmigrated database would push schema-readiness back onto every caller,
/// which is exactly what the type exists to prevent.
#[test]
fn open_in_memory_yields_a_fully_migrated_schema() {
    let db = Db::open_in_memory().unwrap();

    let version: i64 = db
        .conn()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();

    assert_eq!(
        version, SUPPORTED_SCHEMA_VERSION,
        "a fresh handle must already be at the supported schema"
    );
}

/// Worker threads ask the handle where it points so they can open their own —
/// an in-memory database has no file to hand them, and saying so honestly is
/// what lets a caller fall back instead of opening the wrong database.
#[test]
fn path_is_none_for_an_in_memory_database() {
    let db = Db::open_in_memory().unwrap();

    assert_eq!(db.path(), None);
}

/// The counterpart: for a file-backed handle the path must be the file it is
/// actually attached to, not `default_path()` — under a test fixture or an
/// explicitly chosen library those differ, and a worker that assumed the
/// default would quietly write to the user's real library.
#[test]
fn path_reports_the_file_the_handle_is_attached_to() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("library.db");

    let db = Db::open_migrated(Some(&file)).unwrap();

    assert_eq!(db.path().as_deref(), Some(file.as_path()));
}

/// Opening is fallible and the failure must surface as an error, not a panic:
/// `main.rs` turns this into a user-visible message, and a worker thread that
/// cannot open its own connection has to be able to give up quietly.
#[test]
fn open_migrated_reports_an_error_for_an_unusable_path() {
    let dir = tempfile::tempdir().unwrap();
    // A regular file where the database's parent directory would have to be:
    // creating that directory cannot succeed.
    let blocker = dir.path().join("not-a-directory");
    std::fs::write(&blocker, b"").unwrap();

    let result = Db::open_migrated(Some(&blocker.join("library.db")));

    assert!(
        matches!(result, Err(DbError::Io(_))),
        "expected an I/O error, got {result:?}"
    );
}

/// Stateless readers such as MCP open a fresh handle for every request. They
/// must not rerun maintenance, but the handle still cannot admit an
/// unprepared schema.
#[test]
fn open_ready_rejects_an_unmigrated_database_without_migrating_it() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("library.db");
    let raw = db::open(Some(&file)).unwrap();
    drop(raw);

    let result = Db::open_ready(&file);

    assert!(matches!(
        result,
        Err(DbError::SchemaNotReady {
            found: 0,
            supported: SUPPORTED_SCHEMA_VERSION
        })
    ));
    let raw = db::open(Some(&file)).unwrap();
    let version: i64 = raw
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 0, "the read opener must not migrate the database");
}

#[test]
fn open_ready_accepts_an_already_migrated_database() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("library.db");
    drop(Db::open_migrated(Some(&file)).unwrap());

    let db = Db::open_ready(&file).unwrap();

    assert_eq!(db.path().as_deref(), Some(file.as_path()));
}

#[test]
fn open_ready_read_only_accepts_a_migrated_database_without_write_access() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("library.db");
    drop(Db::open_migrated(Some(&file)).unwrap());

    let db = Db::open_ready_read_only(&file).unwrap();

    assert_eq!(db.path().as_deref(), Some(file.as_path()));
    let write = db.conn().execute(
        "INSERT INTO settings (key, value) VALUES ('probe', '1')",
        [],
    );
    assert!(
        write.is_err(),
        "the background prescan handle must stay read-only"
    );
}

#[test]
fn open_ready_read_only_rejects_an_unmigrated_database_without_migrating_it() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("library.db");
    drop(db::open(Some(&file)).unwrap());

    let result = Db::open_ready_read_only(&file);

    assert!(matches!(
        result,
        Err(DbError::SchemaNotReady {
            found: 0,
            supported: SUPPORTED_SCHEMA_VERSION
        })
    ));
}

#[test]
fn open_ready_preserves_prune_eligible_change_log_rows() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("library.db");
    let db = Db::open_migrated(Some(&file)).unwrap();
    db.conn()
        .execute_batch(
            "WITH RECURSIVE numbers(value) AS (
                 SELECT 0
                 UNION ALL
                 SELECT value + 1 FROM numbers WHERE value < 10000
             )
             INSERT INTO change_log (entity, entity_id, op, writer, at)
             SELECT 'scan', CAST(value AS TEXT), 'complete', 1, 1 FROM numbers;",
        )
        .unwrap();
    drop(db);

    let reopened = Db::open_ready(&file).unwrap();
    let count: i64 = reopened
        .conn()
        .query_row("SELECT COUNT(*) FROM change_log", [], |row| row.get(0))
        .unwrap();

    assert_eq!(
        count, 10_001,
        "the stateless read opener must not run open-time pruning"
    );
}

#[test]
fn open_ready_preserves_the_future_schema_error_contract() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("library.db");
    let raw = db::open(Some(&file)).unwrap();
    raw.pragma_update(None, "user_version", SUPPORTED_SCHEMA_VERSION + 1)
        .unwrap();
    drop(raw);

    let result = Db::open_ready(&file);

    assert!(matches!(
        result,
        Err(DbError::SchemaTooNew {
            found,
            supported: SUPPORTED_SCHEMA_VERSION
        }) if found == SUPPORTED_SCHEMA_VERSION + 1
    ));
}
