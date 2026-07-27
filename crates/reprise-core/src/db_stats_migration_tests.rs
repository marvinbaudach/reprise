//! My Stats schema migration regressions and file-backed drills.

use super::recent_migration_tests::open_v11_database;
use super::*;

#[test]
fn migrating_a_v16_database_adds_the_listen_events_track_index() {
    let conn = open_v11_database();
    conn.execute_batch(SCHEMA_V12).unwrap();
    conn.pragma_update(None, "user_version", 12).unwrap();
    conn.execute_batch(SCHEMA_V13).unwrap();
    conn.pragma_update(None, "user_version", 13).unwrap();
    conn.execute_batch(SCHEMA_V14).unwrap();
    conn.pragma_update(None, "user_version", 14).unwrap();
    conn.execute_batch(SCHEMA_V15).unwrap();
    conn.pragma_update(None, "user_version", 15).unwrap();
    let cover_cache = tempfile::tempdir().unwrap();
    let portrait_cache = tempfile::tempdir().unwrap();
    let tx = conn.unchecked_transaction().unwrap();
    grandfather_network_features(&tx, true, cover_cache.path(), portrait_cache.path()).unwrap();
    tx.pragma_update(None, "user_version", 16).unwrap();
    tx.commit().unwrap();

    migrate_with_cache_dirs(&conn, cover_cache.path(), portrait_cache.path()).unwrap();

    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, SUPPORTED_SCHEMA_VERSION);
    let indexes = conn
        .prepare("PRAGMA index_list(listen_events)")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert!(indexes
        .iter()
        .any(|name| name == "idx_listen_events_track_played"));
}

#[test]
fn temporary_file_databases_migrate_from_fresh_and_v16_to_current() {
    let cover_cache = tempfile::tempdir().unwrap();
    let portrait_cache = tempfile::tempdir().unwrap();

    for starting_version in [0, 16] {
        let database = tempfile::Builder::new()
            .prefix("reprise-migration-drill-")
            .suffix(".db")
            .tempfile_in("/tmp")
            .unwrap();
        let conn = Connection::open(database.path()).unwrap();
        migrate_with_cache_dirs(&conn, cover_cache.path(), portrait_cache.path()).unwrap();
        if starting_version == 16 {
            conn.execute_batch(
                "DROP TABLE change_log;
                 DROP TABLE library_exclusions;
                 DROP TABLE library_doctor_remote_cache;
                 DROP TRIGGER tag_write_journal_identity_immutable;
                 DROP TABLE tag_write_journal;
                 DROP TABLE tag_write_job_files;
                 DROP TABLE tag_write_jobs;
                 DROP TABLE library_doctor_state;
                 DROP TABLE library_doctor_group_members;
                 DROP TABLE library_doctor_group_candidates;
                 DROP TABLE library_doctor_groups;
                 DROP TABLE library_doctor_proposals;
                 DROP TABLE library_doctor_scan_tracks;
                 DROP TABLE library_doctor_scans;
                 DROP TABLE ai_jobs;
                 DROP TABLE track_provenance;
                 ALTER TABLE playlists DROP COLUMN role;
                 DROP INDEX idx_listen_events_track_played;
                 ALTER TABLE new_releases DROP COLUMN first_seen;
                 ALTER TABLE new_releases DROP COLUMN hidden_at;
                 ALTER TABLE new_releases DROP COLUMN announce_url;
                 ALTER TABLE new_releases DROP COLUMN track_count;
                 PRAGMA user_version = 16;",
            )
            .unwrap();
            migrate_with_cache_dirs(&conn, cover_cache.path(), portrait_cache.path()).unwrap();
        }

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        let index_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'index' \
                 AND name = 'idx_listen_events_track_played')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            version, SUPPORTED_SCHEMA_VERSION,
            "starting version {starting_version}"
        );
        assert!(index_exists, "starting version {starting_version}");
    }
}
