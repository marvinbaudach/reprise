//! Schema migration for reopening cover discovery under collision detection.

use rusqlite::Connection;

pub(crate) fn migrate_v83(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 83 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute(
        "DELETE FROM settings WHERE key = 'startup_tasks.completed.covers'",
        [],
    )?;
    transaction.pragma_update(None, "user_version", 83)?;
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v83_reopens_only_the_cover_startup_task() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate_connection(&conn).unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, 'cover'), (?2, 'spectrogram')",
            [
                "startup_tasks.completed.covers",
                "startup_tasks.completed.spectrogram",
            ],
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 82).unwrap();

        migrate_v83(&conn).unwrap();

        let remaining = conn
            .prepare("SELECT key FROM settings WHERE key LIKE 'startup_tasks.completed.%'")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(remaining, ["startup_tasks.completed.spectrogram"]);
        assert_eq!(
            conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            83
        );
    }

    #[test]
    fn v83_does_not_repeat_after_the_schema_version_is_current() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate_connection(&conn).unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('startup_tasks.completed.covers', 'new')",
            [],
        )
        .unwrap();

        migrate_v83(&conn).unwrap();

        assert_eq!(
            conn.query_row(
                "SELECT value FROM settings WHERE key = 'startup_tasks.completed.covers'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            "new"
        );
    }
}
