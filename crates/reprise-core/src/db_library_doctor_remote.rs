use rusqlite::Connection;

const SCHEMA_V20: &str = r#"
ALTER TABLE library_doctor_proposals
ADD COLUMN evidence_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE library_doctor_group_candidates
ADD COLUMN evidence_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE library_doctor_proposals
ADD COLUMN local_fallback_json TEXT NOT NULL DEFAULT 'null';
ALTER TABLE library_doctor_groups
ADD COLUMN local_fallback_json TEXT NOT NULL DEFAULT 'null';
"#;

pub(crate) fn migrate_v20(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 20 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(SCHEMA_V20)?;
    transaction.pragma_update(None, "user_version", 20)?;
    transaction.commit()
}

#[cfg(test)]
mod tests {
    #[test]
    fn migration_v19_to_v20_preserves_existing_scans_and_is_idempotent() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO library_doctor_scans \
             (scope_kind, created_at, remote_enabled, checked_tracks, skipped_tracks) \
             VALUES ('whole_library', 1, 0, 0, 0)",
            [],
        )
        .unwrap();
        conn.execute_batch(
            "ALTER TABLE library_doctor_proposals DROP COLUMN evidence_json;
             ALTER TABLE library_doctor_group_candidates DROP COLUMN evidence_json;
             ALTER TABLE library_doctor_proposals DROP COLUMN local_fallback_json;
             ALTER TABLE library_doctor_groups DROP COLUMN local_fallback_json;
             PRAGMA user_version = 19;",
        )
        .unwrap();

        super::migrate_v20(&conn).unwrap();
        super::migrate_v20(&conn).unwrap();

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        let scans: i64 = conn
            .query_row("SELECT COUNT(*) FROM library_doctor_scans", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!((version, scans), (20, 1));
        let defaults = conn
            .prepare("PRAGMA table_info(library_doctor_proposals)")
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, String>(1)?, row.get::<_, Option<String>>(4)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(defaults.contains(&("evidence_json".into(), Some("'[]'".into()))));
        assert!(defaults.contains(&("local_fallback_json".into(), Some("'null'".into()))));

        conn.execute(
            "INSERT INTO library_doctor_groups \
             (scan_id, position, field, group_key) VALUES (1, 0, 'year', 'remote:year')",
            [],
        )
        .unwrap();
        let group_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO library_doctor_group_candidates \
             (group_id, position, candidate_value, candidate_count, evidence_json) \
             VALUES (?1, 0, '2024', 1, '[{\"source\":\"music_brainz\",\"confidence\":80}]')",
            [group_id],
        )
        .unwrap();
        conn.execute("DELETE FROM library_doctor_scans WHERE id=1", [])
            .unwrap();
        let candidates: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM library_doctor_group_candidates",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(candidates, 0);
    }
}
