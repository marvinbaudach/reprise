//! Header-case migration for the three built-in smart playlist names.

use rusqlite::Connection;

pub(crate) fn migrate_v85(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version >= 85 {
        return Ok(());
    }
    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(
        r#"
UPDATE smart_playlists
SET name = 'Recently Played'
WHERE name = 'Recently played'
  AND rules_json = '[{"field":"last_played_at","op":"not-null"}]';
UPDATE smart_playlists
SET name = 'Top Rated'
WHERE name = 'Top rated'
  AND rules_json = '[{"field":"rating","op":">=","value":4}]';
UPDATE smart_playlists
SET name = 'Recently Added'
WHERE name = 'Recently added'
  AND role = 'recently_added';
"#,
    )?;
    transaction.pragma_update(None, "user_version", 85)?;
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v85_renames_only_the_three_builtin_smart_playlists() {
        let conn = crate::db::open_migrated(None).unwrap();
        conn.execute_batch(
            "UPDATE smart_playlists SET name = 'Recently played' WHERE name = 'Recently Played';
             UPDATE smart_playlists SET name = 'Top rated' WHERE name = 'Top Rated';
             UPDATE smart_playlists SET name = 'Recently added' WHERE name = 'Recently Added';",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO smart_playlists \
             (name, rules_json, sort_field, sort_dir, limit_count) \
             VALUES ('Top rated', '[{\"field\":\"genre\",\"op\":\"=\",\"value\":\"Rock\"}]', \
                     'title', 'asc', NULL)",
            [],
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 84).unwrap();

        migrate_v85(&conn).unwrap();
        migrate_v85(&conn).unwrap();

        let rows = conn
            .prepare("SELECT name, rules_json, role FROM smart_playlists ORDER BY id")
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(
            rows,
            [
                (
                    "Recently Played".to_owned(),
                    r#"[{"field":"last_played_at","op":"not-null"}]"#.to_owned(),
                    None,
                ),
                (
                    "Top Rated".to_owned(),
                    r#"[{"field":"rating","op":">=","value":4}]"#.to_owned(),
                    None,
                ),
                (
                    "Recently Added".to_owned(),
                    "[]".to_owned(),
                    Some("recently_added".to_owned()),
                ),
                (
                    "Top rated".to_owned(),
                    r#"[{"field":"genre","op":"=","value":"Rock"}]"#.to_owned(),
                    None,
                ),
            ]
        );
        assert_eq!(
            conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            85
        );
    }
}
