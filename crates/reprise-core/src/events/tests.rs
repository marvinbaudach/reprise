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
