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
fn schema_v28_fresh_and_v26_upgrade_have_the_same_change_log_shape() {
    let fresh = open(None).unwrap();
    migrate_connection(&fresh).unwrap();
    let expected = change_log_schema(&fresh);

    let upgraded = open(None).unwrap();
    migrate_connection(&upgraded).unwrap();
    // Roll back to before change_log (now v28). Every object created at v28+
    // must go, or re-migration's later steps collide with the survivors — the
    // v29 AI-jobs shape (ai_jobs/track_provenance/playlists.role) included. The
    // v26 new_releases-history columns stay, so re-migration resumes at v26 and
    // replays v27 (the audio-analysis drop, a no-op here) through v29.
    upgraded
        .execute_batch(
            "DROP TABLE change_log;
             DROP TABLE ai_jobs;
             DROP TABLE track_provenance;
             ALTER TABLE playlists DROP COLUMN role;
             ALTER TABLE new_releases DROP COLUMN track_count;
             PRAGMA user_version = 26;",
        )
        .unwrap();
    migrate_connection(&upgraded).unwrap();

    assert_eq!(change_log_schema(&upgraded), expected);
    assert_eq!(
        upgraded
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        SUPPORTED_SCHEMA_VERSION
    );
}

#[test]
fn schema_v28_change_log_has_the_ordering_and_lookup_contract() {
    let conn = open(None).unwrap();
    migrate_connection(&conn).unwrap();

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
    // Version-agnostic: one past whatever this binary supports is always
    // "too new", so this guard test survives every future schema bump.
    let too_new = SUPPORTED_SCHEMA_VERSION + 1;
    let conn = open(None).unwrap();
    conn.pragma_update(None, "user_version", too_new).unwrap();

    let error = migrate_connection(&conn).unwrap_err();

    assert!(matches!(
        error,
        DbError::SchemaTooNew { found, supported }
            if found == too_new && supported == SUPPORTED_SCHEMA_VERSION
    ));
}

#[test]
fn open_migrated_rejects_a_schema_newer_than_the_binary_supports() {
    let too_new = SUPPORTED_SCHEMA_VERSION + 1;
    let database = tempfile::NamedTempFile::new().unwrap();
    let conn = Connection::open(database.path()).unwrap();
    conn.pragma_update(None, "user_version", too_new).unwrap();
    drop(conn);

    let error = open_migrated(Some(database.path())).unwrap_err();

    assert!(matches!(
        error,
        DbError::SchemaTooNew { found, supported }
            if found == too_new && supported == SUPPORTED_SCHEMA_VERSION
    ));
}
