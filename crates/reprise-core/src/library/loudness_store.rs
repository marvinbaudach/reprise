use rusqlite::Connection;

pub(crate) fn migrate_v88(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 88 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    for (column, declaration) in [
        ("rg_track_gain", "REAL"),
        ("rg_track_peak", "REAL"),
        ("rg_album_gain", "REAL"),
        ("rg_album_peak", "REAL"),
        ("tag_scan_version", "INTEGER NOT NULL DEFAULT 0"),
    ] {
        let exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('tracks') WHERE name = ?1)",
            [column],
            |row| row.get(0),
        )?;
        if !exists {
            transaction.execute_batch(&format!(
                "ALTER TABLE tracks ADD COLUMN {column} {declaration};"
            ))?;
        }
    }
    transaction.pragma_update(None, "user_version", 88)?;
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_v88_adds_replaygain_columns_once() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE tracks (id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE);\
             PRAGMA user_version = 87;",
        )
        .unwrap();

        migrate_v88(&conn).unwrap();

        for column in [
            "rg_track_gain",
            "rg_track_peak",
            "rg_album_gain",
            "rg_album_peak",
            "tag_scan_version",
        ] {
            let exists: bool = conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM pragma_table_info('tracks') WHERE name = ?1)",
                    [column],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(exists, "missing tracks.{column}");
        }
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 88);

        migrate_v88(&conn).unwrap();
    }
}
