use super::*;

fn change_log_schema(conn: &Connection) -> Vec<(String, String)> {
    let mut statement = conn
        .prepare(
            "SELECT name, sql FROM sqlite_schema \
             WHERE (type = 'table' AND name = 'change_log') \
                OR (type = 'index' AND tbl_name = 'change_log') \
             ORDER BY type, name",
        )
        .unwrap();
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

#[test]
fn schema_v26_fresh_and_v25_upgrade_have_the_same_change_log_shape() {
    let fresh = open(None).unwrap();
    migrate(&fresh).unwrap();
    let expected = change_log_schema(&fresh);

    let upgraded = open(None).unwrap();
    migrate(&upgraded).unwrap();
    upgraded
        .execute_batch(
            "DROP TABLE change_log;
             PRAGMA user_version = 25;",
        )
        .unwrap();
    migrate(&upgraded).unwrap();

    assert_eq!(change_log_schema(&upgraded), expected);
    assert_eq!(
        upgraded
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        26
    );
}

#[test]
fn schema_v26_change_log_has_the_ordering_and_lookup_contract() {
    let conn = open(None).unwrap();
    migrate(&conn).unwrap();

    let columns = conn
        .prepare("PRAGMA table_info(change_log)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(columns, ["id", "entity", "entity_id", "op", "writer", "at"]);
    assert!(change_log_schema(&conn)
        .iter()
        .any(|(name, _)| name == "idx_change_log_at"));
}

#[test]
fn migrate_rejects_a_schema_newer_than_the_binary_supports() {
    let conn = open(None).unwrap();
    conn.pragma_update(None, "user_version", 27).unwrap();

    let error = migrate(&conn).unwrap_err();

    assert!(matches!(
        error,
        DbError::SchemaTooNew {
            found: 27,
            supported: 26
        }
    ));
}

#[test]
fn open_migrated_rejects_a_schema_newer_than_the_binary_supports() {
    let database = tempfile::NamedTempFile::new().unwrap();
    let conn = Connection::open(database.path()).unwrap();
    conn.pragma_update(None, "user_version", 27).unwrap();
    drop(conn);

    let error = open_migrated(Some(database.path())).unwrap_err();

    assert!(matches!(
        error,
        DbError::SchemaTooNew {
            found: 27,
            supported: 26
        }
    ));
}
