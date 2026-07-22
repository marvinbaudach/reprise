use super::*;

fn migrated_conn() -> rusqlite::Connection {
    crate::db::open_migrated(None).unwrap()
}

#[test]
fn rolled_back_domain_transaction_leaves_no_change_event() {
    let mut conn = migrated_conn();
    {
        let transaction = conn.transaction().unwrap();
        record_at(
            &transaction,
            "playlist",
            "41",
            "create",
            WriterToken(7),
            1_000,
        )
        .unwrap();
    }

    assert!(read_since(&conn, 0, None).unwrap().is_empty());
}

#[test]
fn committed_events_are_read_in_total_id_order() {
    let conn = migrated_conn();
    record_at(&conn, "playlist", "2", "rename", WriterToken(7), 2_000).unwrap();
    record_at(&conn, "settings", "density", "set", WriterToken(8), 1_000).unwrap();

    let changes = read_since(&conn, 0, None).unwrap();

    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0].id + 1, changes[1].id);
    assert_eq!(changes[0].entity, "playlist");
    assert_eq!(changes[1].entity_id, "density");
    assert_eq!(changes[1].operation, "set");
}

#[test]
fn read_since_excludes_the_callers_writer_token() {
    let conn = migrated_conn();
    record_at(&conn, "playlist", "1", "create", WriterToken(11), 1).unwrap();
    record_at(&conn, "playlist", "2", "create", WriterToken(12), 2).unwrap();

    let changes = read_since(&conn, 0, Some(WriterToken(11))).unwrap();

    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].entity_id, "2");
    assert_eq!(changes[0].writer, WriterToken(12));
}

#[test]
fn writer_token_is_stable_for_the_process() {
    assert_eq!(writer_token(), writer_token());
}

#[test]
fn record_uses_the_process_writer_token() {
    let conn = migrated_conn();

    record(&conn, "playlist", "1", "create").unwrap();

    let changes = read_since(&conn, 0, None).unwrap();
    assert_eq!(changes[0].writer, writer_token());
}

#[test]
fn prune_keeps_recent_rows_even_beyond_the_count_floor() {
    let conn = migrated_conn();
    for id in 0..=MAX_RETAINED_CHANGES {
        record_at(
            &conn,
            "scan",
            &id.to_string(),
            "complete",
            WriterToken(1),
            10_000,
        )
        .unwrap();
    }

    assert_eq!(prune_at(&conn, 10_000).unwrap(), 0);
    assert_eq!(read_since(&conn, 0, None).unwrap().len(), 10_001);
}

#[test]
fn prune_removes_only_rows_older_than_both_retention_boundaries() {
    let conn = migrated_conn();
    for id in 0..=MAX_RETAINED_CHANGES {
        record_at(
            &conn,
            "scan",
            &id.to_string(),
            "complete",
            WriterToken(1),
            1,
        )
        .unwrap();
    }

    assert_eq!(prune_at(&conn, RETENTION_SECS + 2).unwrap(), 1);
    let remaining = read_since(&conn, 0, None).unwrap();
    assert_eq!(remaining.len(), MAX_RETAINED_CHANGES);
    assert_eq!(remaining.first().unwrap().entity_id, "1");
}

#[test]
fn open_migrated_prunes_the_persisted_change_log() {
    let database = tempfile::NamedTempFile::new().unwrap();
    let conn = crate::db::open_migrated(Some(database.path())).unwrap();
    for id in 0..=MAX_RETAINED_CHANGES {
        record_at(
            &conn,
            "scan",
            &id.to_string(),
            "complete",
            WriterToken(1),
            1,
        )
        .unwrap();
    }
    drop(conn);

    let reopened = crate::db::open_migrated(Some(database.path())).unwrap();

    assert_eq!(read_since(&reopened, 0, None).unwrap().len(), 10_000);
}

/// F1 regression (a): `open_migrated` must return a usable connection even while
/// another connection holds an open write transaction — the prune skips rather
/// than blocking out the 5s busy_timeout and panicking at the `.unwrap()` call
/// sites. Before the fix this open blocked ~5s and then failed.
#[test]
fn open_migrated_succeeds_while_a_foreign_write_transaction_is_held() {
    let database = tempfile::NamedTempFile::new().unwrap();
    // Populate enough old rows that a prune is genuinely due, so the open path
    // actually reaches the (now non-blocking) DELETE rather than the idle probe.
    let conn = crate::db::open_migrated(Some(database.path())).unwrap();
    for id in 0..=MAX_RETAINED_CHANGES {
        record_at(&conn, "scan", &id.to_string(), "complete", WriterToken(1), 1).unwrap();
    }
    drop(conn);

    // Hold an exclusive write transaction on a second connection.
    let blocker = crate::db::open(Some(database.path())).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();

    let started = std::time::Instant::now();
    let reopened = crate::db::open_migrated(Some(database.path())).unwrap();
    assert!(
        started.elapsed() < std::time::Duration::from_secs(4),
        "open must not block waiting out the busy_timeout on the held write lock"
    );
    // The contended prune skipped, leaving the eligible row in place; the
    // connection is fully usable regardless.
    assert_eq!(read_since(&reopened, 0, None).unwrap().len(), 10_001);

    blocker.execute_batch("ROLLBACK").unwrap();
}

/// F1 regression (b): opening a database with nothing to prune performs no
/// write at all. Proven via `PRAGMA data_version`, which advances only when
/// *another* connection commits — an independent observer must see it unchanged
/// across the idle reopen.
#[test]
fn open_migrated_with_nothing_to_prune_performs_no_write() {
    let database = tempfile::NamedTempFile::new().unwrap();
    let conn = crate::db::open_migrated(Some(database.path())).unwrap();
    // One recent row: below both retention floors, so nothing is ever eligible.
    record(&conn, "playlist", "1", "create").unwrap();
    drop(conn);

    let observer = crate::db::open(Some(database.path())).unwrap();
    let before: i64 = observer
        .query_row("PRAGMA data_version", [], |row| row.get(0))
        .unwrap();

    let reopened = crate::db::open_migrated(Some(database.path())).unwrap();
    // Read the counter while `reopened` is still alive: a would-be prune write
    // happens during the open above, so it would already show here.
    let after: i64 = observer
        .query_row("PRAGMA data_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        before, after,
        "an idle reopen must not commit any change-log write"
    );
    drop(reopened);
}
