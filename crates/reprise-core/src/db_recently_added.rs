//! Semantic identity migration for the built-in Recently Added Library scope.

use rusqlite::Connection;

pub(crate) fn migrate_v35(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 35 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    let has_role = {
        let mut statement = transaction.prepare("PRAGMA table_info(smart_playlists)")?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        columns.into_iter().any(|column| column == "role")
    };
    if !has_role {
        transaction.execute("ALTER TABLE smart_playlists ADD COLUMN role TEXT", [])?;
    }
    transaction.execute_batch(
        "UPDATE smart_playlists
         SET role = 'recently_added', limit_count = NULL
         WHERE name = 'Recently added' AND rules_json = '[]';
         CREATE UNIQUE INDEX IF NOT EXISTS idx_smart_playlists_role
         ON smart_playlists(role) WHERE role IS NOT NULL;",
    )?;
    transaction.pragma_update(None, "user_version", 35)?;
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fil_8_v35_marks_the_builtin_scope_and_removes_its_cap() {
        let conn = crate::db::open_migrated(None).unwrap();
        let row: (Option<String>, Option<i64>) = conn
            .query_row(
                "SELECT role, limit_count FROM smart_playlists
                 WHERE name = 'Recently added'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(row, (Some("recently_added".into()), None));

        migrate_v35(&conn).unwrap();
        assert_eq!(
            conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            crate::db::SUPPORTED_SCHEMA_VERSION
        );
    }
}
