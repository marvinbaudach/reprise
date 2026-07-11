//! Manual and smart playlist management.
//!
//! Manual playlists store ordered track references (duplicates allowed, like
//! Rhythmbox). Smart playlists filter tracks via a rules JSON document with
//! sort and limit options. Both types maintain gapless 0-indexed positions
//! across all operations (create, add, remove, move).

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// Summary of a manual playlist (name, id, track count).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaylistSummary {
    pub id: i64,
    pub name: String,
    pub track_count: i64,
}

/// Smart playlist definition: rules (field/op/value, AND-joined), sort order,
/// and optional limit.
#[derive(Debug, Clone)]
pub struct SmartPlaylist {
    pub id: i64,
    /// Displayed by `ui::sidebar` (Task 4) alongside `list`'s
    /// `PlaylistSummary.name`; `queries.rs`'s `ViewSource::Smart` handling
    /// itself only needs `rules_json`/`sort_field`/`sort_dir`/`limit_count`.
    pub name: String,
    pub rules_json: String,
    pub sort_field: String,
    pub sort_dir: String,
    pub limit_count: Option<i64>,
}

/// Custom error for smart playlist rule parsing and validation.
#[derive(Debug, thiserror::Error)]
pub enum SmartRulesError {
    #[error("JSON parse error: {0}")]
    JsonParse(#[from] serde_json::Error),
    #[error("unknown field: {0}")]
    UnknownField(String),
    #[error("unknown operator: {0}")]
    UnknownOperator(String),
    #[error("operator {0} requires a value")]
    MissingValue(String),
    #[error("invalid value for operator {0}")]
    InvalidValue(String),
}

/// Single rule in the smart playlist rules array: { field, op, value? }.
#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct Rule {
    field: String,
    op: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<serde_json::Value>,
}

/// Creates a manual playlist with the given name and returns its new id.
/// Positions are assigned sequentially (new playlist gets `max(position) + 1`).
/// Empty or whitespace-only name is accepted (backend is dumb; UI validates).
pub fn create(conn: &Connection, name: &str) -> Result<i64, rusqlite::Error> {
    create_playlist_row(conn, name)
}

/// Shared insert logic behind [`create`] and [`create_with_tracks`] — takes a
/// plain `&Connection` so it can run either standalone (`create`) or against
/// a `&Transaction` via deref coercion (`create_with_tracks`), without
/// nesting a second `BEGIN` inside an already-open transaction.
fn create_playlist_row(conn: &Connection, name: &str) -> Result<i64, rusqlite::Error> {
    let trimmed = name.trim();
    let insert_name = if trimmed.is_empty() {
        name.to_string()
    } else {
        trimmed.to_string()
    };

    let max_position: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(position), -1) FROM playlists",
            [],
            |r| r.get(0),
        )
        .unwrap_or(-1);
    let new_position = max_position + 1;

    conn.execute(
        "INSERT INTO playlists (name, position) VALUES (?1, ?2)",
        params![insert_name, new_position],
    )?;

    let id = conn.last_insert_rowid();
    Ok(id)
}

/// Renames a playlist by id.
#[allow(dead_code)]
pub fn rename(conn: &Connection, id: i64, name: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE playlists SET name = ?1 WHERE id = ?2",
        params![name, id],
    )?;
    Ok(())
}

/// Deletes a playlist by id. Cascades to all its playlist_tracks rows.
#[allow(dead_code)]
pub fn delete(conn: &Connection, id: i64) -> Result<(), rusqlite::Error> {
    conn.execute("DELETE FROM playlists WHERE id = ?1", params![id])?;
    Ok(())
}

/// Lists all manual playlists, ordered by position (ascending).
/// Includes track count for each (0 if empty).
pub fn list(conn: &Connection) -> Result<Vec<PlaylistSummary>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.name, COALESCE(COUNT(pt.track_id), 0) as track_count \
         FROM playlists p \
         LEFT JOIN playlist_tracks pt ON p.id = pt.playlist_id \
         GROUP BY p.id \
         ORDER BY p.position ASC",
    )?;
    let playlists = stmt.query_map([], |row| {
        Ok(PlaylistSummary {
            id: row.get(0)?,
            name: row.get(1)?,
            track_count: row.get(2)?,
        })
    })?;

    let mut result = Vec::new();
    for playlist in playlists {
        result.push(playlist?);
    }
    Ok(result)
}

/// Appends tracks to a playlist at the end (appends to the highest position).
/// Positions are contiguous. Duplicates allowed (Rhythmbox behavior).
/// All inserts happen in one transaction.
#[allow(dead_code)]
pub fn add_tracks(
    conn: &mut Connection,
    playlist_id: i64,
    track_ids: &[i64],
) -> Result<u32, rusqlite::Error> {
    if track_ids.is_empty() {
        return Ok(0);
    }

    let tx = conn.transaction()?;
    let inserted = append_tracks_rows(&tx, playlist_id, track_ids)?;
    tx.commit()?;
    Ok(inserted)
}

/// Shared per-row append logic behind [`add_tracks`] and
/// [`create_with_tracks`] — see [`create_playlist_row`]'s doc comment for why
/// this takes a plain `&Connection` rather than managing its own transaction.
/// Caller is responsible for the empty-slice short circuit (both callers
/// already have their own reason to check it first).
fn append_tracks_rows(
    conn: &Connection,
    playlist_id: i64,
    track_ids: &[i64],
) -> Result<u32, rusqlite::Error> {
    let max_position: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(position), -1) FROM playlist_tracks WHERE playlist_id = ?1",
            params![playlist_id],
            |r| r.get(0),
        )
        .unwrap_or(-1);

    let mut inserted = 0u32;
    for (i, &track_id) in track_ids.iter().enumerate() {
        let position = max_position + 1 + i as i64;
        conn.execute(
            "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (?1, ?2, ?3)",
            params![playlist_id, track_id, position],
        )?;
        inserted += 1;
    }
    Ok(inserted)
}

/// Atomically creates a manual playlist named `name` and appends `track_ids`
/// to it in a single transaction: create → append → commit, with any failure
/// (e.g. a `track_id` that violates the `playlist_tracks.track_id` foreign
/// key) rolling back *both* — no orphaned empty playlist row is left behind.
/// Prefer this over a separate `create` + `add_tracks` pair whenever the two
/// must succeed or fail together (e.g. M3U import — see `ui::playlist_io::
/// import_playlist`). Existing callers that don't need this guarantee (e.g.
/// "New playlist" from the track list context menu) keep using `create` and
/// `add_tracks` directly.
pub fn create_with_tracks(
    conn: &mut Connection,
    name: &str,
    track_ids: &[i64],
) -> Result<i64, rusqlite::Error> {
    let tx = conn.transaction()?;
    let playlist_id = create_playlist_row(&tx, name)?;
    if !track_ids.is_empty() {
        append_tracks_rows(&tx, playlist_id, track_ids)?;
    }
    tx.commit()?;
    Ok(playlist_id)
}

/// Removes tracks at the specified positions and renumbers the remaining
/// tracks to keep positions contiguous (0..n-1). Multiple removes happen in
/// one transaction.
#[allow(dead_code)]
pub fn remove_positions(
    conn: &mut Connection,
    playlist_id: i64,
    positions: &[u32],
) -> Result<u32, rusqlite::Error> {
    if positions.is_empty() {
        return Ok(0);
    }

    let tx = conn.transaction()?;

    // Convert positions to a set for efficient lookup.
    let positions_set: std::collections::HashSet<u32> = positions.iter().copied().collect();

    // Delete all rows at the specified positions.
    let mut deleted = 0u32;
    for &pos in &positions_set {
        let changes = tx.execute(
            "DELETE FROM playlist_tracks WHERE playlist_id = ?1 AND position = ?2",
            params![playlist_id, pos as i64],
        )?;
        deleted += changes as u32;
    }

    // Renumber remaining tracks to be contiguous.
    let mut stmt = tx.prepare(
        "SELECT position FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position ASC",
    )?;
    let current_positions: Vec<i64> = stmt
        .query_map(params![playlist_id], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    for (new_pos, &old_pos) in current_positions.iter().enumerate() {
        if old_pos != new_pos as i64 {
            tx.execute(
                "UPDATE playlist_tracks SET position = ?1 WHERE playlist_id = ?2 AND position = ?3",
                params![new_pos as i64, playlist_id, old_pos],
            )?;
        }
    }

    tx.commit()?;
    Ok(deleted)
}

/// Moves a track from one position to another, renumbering all affected
/// positions to stay contiguous (0..n-1). If `from` or `to` are out of range,
/// logs a warning and returns Ok (no-op). One transaction.
#[allow(dead_code)]
pub fn move_position(
    conn: &mut Connection,
    playlist_id: i64,
    from: u32,
    to: u32,
) -> Result<(), rusqlite::Error> {
    let max_position: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(position), -1) FROM playlist_tracks WHERE playlist_id = ?1",
            params![playlist_id],
            |r| r.get(0),
        )
        .unwrap_or(-1);

    if from as i64 > max_position || to as i64 > max_position {
        tracing::warn!(
            playlist_id = playlist_id,
            from = from,
            to = to,
            max = max_position,
            "move_position: out of range, no-op"
        );
        return Ok(());
    }

    if from == to {
        return Ok(()); // no-op
    }

    let tx = conn.transaction()?;

    // Fetch all (track_id, position) pairs in order.
    let mut stmt = tx.prepare(
        "SELECT track_id, position FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
    )?;
    let mut tracks: Vec<i64> = stmt
        .query_map(params![playlist_id], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    if from as usize >= tracks.len() {
        tracing::warn!(
            playlist_id = playlist_id,
            from = from,
            len = tracks.len(),
            "move_position: from position out of range"
        );
        return Ok(());
    }

    // Remove track from its current position and insert at new position.
    let track_id = tracks.remove(from as usize);
    tracks.insert(to as usize, track_id);

    // Delete all rows for this playlist and re-insert with new positions.
    tx.execute(
        "DELETE FROM playlist_tracks WHERE playlist_id = ?1",
        params![playlist_id],
    )?;

    // Re-insert all tracks with updated positions.
    for (new_pos, track_id_val) in tracks.iter().enumerate() {
        tx.execute(
            "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (?1, ?2, ?3)",
            params![playlist_id, track_id_val, new_pos as i64],
        )?;
    }

    tx.commit()?;
    Ok(())
}

/// Converts smart playlist rules (JSON array of {field, op, value?}) into a
/// WHERE clause fragment and parameter list for use in a SELECT statement.
/// Rules are AND-joined. Each rule is validated against whitelisted fields
/// and operators; unknown field/op returns an error (never silently passes
/// through — SQL injection surface).
pub fn smart_rules_to_sql(
    rules_json: &str,
) -> Result<(String, Vec<rusqlite::types::Value>), SmartRulesError> {
    if rules_json.trim().is_empty() || rules_json.trim() == "[]" {
        return Ok(("1=1".to_string(), vec![]));
    }

    let rules: Vec<Rule> = serde_json::from_str(rules_json)?;

    // Whitelisted fields.
    let valid_fields = vec![
        "title",
        "artist",
        "album",
        "genre",
        "year",
        "rating",
        "play_count",
        "last_played_at",
        "added_at",
        "duration_ms",
    ];

    // Whitelisted operators.
    let valid_ops = vec![
        "=", "!=", ">=", "<=", ">", "<", "contains", "not-null", "is-null",
    ];

    let mut where_parts = Vec::new();
    let mut params = Vec::new();

    for rule in rules {
        // Validate field.
        if !valid_fields.contains(&rule.field.as_str()) {
            return Err(SmartRulesError::UnknownField(rule.field));
        }

        // Validate operator.
        if !valid_ops.contains(&rule.op.as_str()) {
            return Err(SmartRulesError::UnknownOperator(rule.op));
        }

        // Build the WHERE fragment for this rule.
        let where_frag = match rule.op.as_str() {
            "=" => {
                let val = rule
                    .value
                    .ok_or_else(|| SmartRulesError::MissingValue("=".to_string()))?;
                params.push(json_value_to_sql(&val));
                format!("{} = ?", rule.field)
            }
            "!=" => {
                let val = rule
                    .value
                    .ok_or_else(|| SmartRulesError::MissingValue("!=".to_string()))?;
                params.push(json_value_to_sql(&val));
                format!("{} != ?", rule.field)
            }
            ">=" => {
                let val = rule
                    .value
                    .ok_or_else(|| SmartRulesError::MissingValue(">=".to_string()))?;
                params.push(json_value_to_sql(&val));
                format!("{} >= ?", rule.field)
            }
            "<=" => {
                let val = rule
                    .value
                    .ok_or_else(|| SmartRulesError::MissingValue("<=".to_string()))?;
                params.push(json_value_to_sql(&val));
                format!("{} <= ?", rule.field)
            }
            ">" => {
                let val = rule
                    .value
                    .ok_or_else(|| SmartRulesError::MissingValue(">".to_string()))?;
                params.push(json_value_to_sql(&val));
                format!("{} > ?", rule.field)
            }
            "<" => {
                let val = rule
                    .value
                    .ok_or_else(|| SmartRulesError::MissingValue("<".to_string()))?;
                params.push(json_value_to_sql(&val));
                format!("{} < ?", rule.field)
            }
            "contains" => {
                let val = rule
                    .value
                    .ok_or_else(|| SmartRulesError::MissingValue("contains".to_string()))?;
                let s = val
                    .as_str()
                    .ok_or_else(|| SmartRulesError::InvalidValue("contains".to_string()))?;
                // Escape backslash first, then % and _ for LIKE.
                let escaped = s
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_");
                params.push(rusqlite::types::Value::Text(format!("%{escaped}%")));
                format!("{} LIKE ? ESCAPE '\\'", rule.field)
            }
            "not-null" => {
                format!("{} IS NOT NULL", rule.field)
            }
            "is-null" => {
                format!("{} IS NULL", rule.field)
            }
            _ => unreachable!(),
        };

        where_parts.push(where_frag);
    }

    let where_clause = if where_parts.is_empty() {
        "1=1".to_string()
    } else {
        where_parts.join(" AND ")
    };

    Ok((where_clause, params))
}

/// Converts a serde_json Value to a rusqlite Value.
#[allow(dead_code)]
fn json_value_to_sql(v: &serde_json::Value) -> rusqlite::types::Value {
    match v {
        serde_json::Value::Null => rusqlite::types::Value::Null,
        serde_json::Value::Bool(b) => rusqlite::types::Value::Integer(if *b { 1 } else { 0 }),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                rusqlite::types::Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                rusqlite::types::Value::Real(f)
            } else {
                rusqlite::types::Value::Text(n.to_string())
            }
        }
        serde_json::Value::String(s) => rusqlite::types::Value::Text(s.clone()),
        _ => rusqlite::types::Value::Text(v.to_string()),
    }
}

/// Lists all smart playlists.
pub fn list_smart(conn: &Connection) -> Result<Vec<SmartPlaylist>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, name, rules_json, sort_field, sort_dir, limit_count FROM smart_playlists",
    )?;
    let smart_playlists = stmt.query_map([], |row| {
        Ok(SmartPlaylist {
            id: row.get(0)?,
            name: row.get(1)?,
            rules_json: row.get(2)?,
            sort_field: row.get(3)?,
            sort_dir: row.get(4)?,
            limit_count: row.get(5)?,
        })
    })?;

    let mut result = Vec::new();
    for playlist in smart_playlists {
        result.push(playlist?);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded_conn() -> Connection {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        // Insert test tracks.
        for id in 1..=5 {
            conn.execute(
                "INSERT INTO tracks (id, path, title, artist, added_at, rating, play_count, last_played_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    id,
                    format!("/x/track{}.flac", id),
                    format!("Track {}", id),
                    format!("Artist {}", id),
                    1000 + id,
                    (id % 5) as i64,           // rating 0-4
                    id * 10,                    // play_count
                    if id > 2 { Some(2000 + id) } else { None }, // last_played_at
                ],
            )
            .unwrap();
        }
        conn
    }

    #[test]
    fn create_playlist_returns_new_id() {
        let conn = seeded_conn();
        let id = create(&conn, "My Playlist").unwrap();
        assert!(id > 0);

        let name: String = conn
            .query_row(
                "SELECT name FROM playlists WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(name, "My Playlist");
    }

    #[test]
    fn create_playlist_assigns_sequential_positions() {
        let conn = seeded_conn();
        let id1 = create(&conn, "Playlist 1").unwrap();
        let id2 = create(&conn, "Playlist 2").unwrap();
        let id3 = create(&conn, "Playlist 3").unwrap();

        let (pos1, pos2, pos3): (i64, i64, i64) = conn
            .query_row(
                "SELECT position FROM playlists WHERE id = ?1",
                params![id1],
                |r| r.get(0),
            )
            .and_then(|p1| {
                let p2: i64 = conn.query_row(
                    "SELECT position FROM playlists WHERE id = ?1",
                    params![id2],
                    |r| r.get(0),
                )?;
                let p3: i64 = conn.query_row(
                    "SELECT position FROM playlists WHERE id = ?1",
                    params![id3],
                    |r| r.get(0),
                )?;
                Ok((p1, p2, p3))
            })
            .unwrap();

        assert_eq!(pos1, 0);
        assert_eq!(pos2, 1);
        assert_eq!(pos3, 2);
    }

    #[test]
    fn create_playlist_accepts_empty_name() {
        let conn = seeded_conn();
        let id = create(&conn, "").unwrap();
        let name: String = conn
            .query_row(
                "SELECT name FROM playlists WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(name, "");
    }

    #[test]
    fn create_playlist_trims_whitespace_name() {
        let conn = seeded_conn();
        let id = create(&conn, "  My Playlist  ").unwrap();
        let name: String = conn
            .query_row(
                "SELECT name FROM playlists WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(name, "My Playlist");
    }

    #[test]
    fn rename_playlist() {
        let conn = seeded_conn();
        let id = create(&conn, "Old Name").unwrap();
        rename(&conn, id, "New Name").unwrap();

        let name: String = conn
            .query_row(
                "SELECT name FROM playlists WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(name, "New Name");
    }

    #[test]
    fn delete_playlist() {
        let conn = seeded_conn();
        let id = create(&conn, "To Delete").unwrap();
        delete(&conn, id).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM playlists WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn list_playlists_ordered_by_position() {
        let conn = seeded_conn();
        let _ = create(&conn, "P1").unwrap();
        let _ = create(&conn, "P2").unwrap();
        let _ = create(&conn, "P3").unwrap();

        let playlists = list(&conn).unwrap();
        assert_eq!(playlists.len(), 3);
        assert_eq!(playlists[0].name, "P1");
        assert_eq!(playlists[1].name, "P2");
        assert_eq!(playlists[2].name, "P3");
    }

    #[test]
    fn list_playlists_includes_track_count() {
        let mut m_conn = crate::db::open(None).unwrap();
        crate::db::migrate(&m_conn).unwrap();
        // Set up same test data
        for id in 1..=5 {
            m_conn
                .execute(
                    "INSERT INTO tracks (id, path, title, artist, added_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        id,
                        format!("/x/track{}.flac", id),
                        format!("Track {}", id),
                        format!("Artist {}", id),
                        1000 + id,
                    ],
                )
                .unwrap();
        }
        let id = create(&m_conn, "P1").unwrap();
        add_tracks(&mut m_conn, id, &[1, 2, 3]).unwrap();

        let playlists = list(&m_conn).unwrap();
        assert_eq!(playlists.len(), 1);
        assert_eq!(playlists[0].track_count, 3);
    }

    #[test]
    fn add_tracks_appends_to_end() {
        let mut conn = seeded_conn();
        let id = create(&conn, "P1").unwrap();
        let inserted = add_tracks(&mut conn, id, &[1, 2, 3]).unwrap();
        assert_eq!(inserted, 3);

        let positions: Vec<i64> = conn
            .prepare(
                "SELECT position FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
            )
            .unwrap()
            .query_map(params![id], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(positions, vec![0, 1, 2]);
    }

    #[test]
    fn add_tracks_empty_slice_returns_zero() {
        let mut conn = seeded_conn();
        let id = create(&conn, "P1").unwrap();
        let inserted = add_tracks(&mut conn, id, &[]).unwrap();
        assert_eq!(inserted, 0);
    }

    #[test]
    fn add_tracks_allows_duplicates() {
        let mut conn = seeded_conn();
        let id = create(&conn, "P1").unwrap();
        add_tracks(&mut conn, id, &[1, 2, 1, 3, 1]).unwrap();

        let track_ids: Vec<i64> = conn
            .prepare(
                "SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
            )
            .unwrap()
            .query_map(params![id], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(track_ids, vec![1, 2, 1, 3, 1]);
    }

    #[test]
    fn add_tracks_multiple_calls_append() {
        let mut conn = seeded_conn();
        let id = create(&conn, "P1").unwrap();
        add_tracks(&mut conn, id, &[1, 2]).unwrap();
        add_tracks(&mut conn, id, &[3, 4]).unwrap();

        let track_ids: Vec<i64> = conn
            .prepare(
                "SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
            )
            .unwrap()
            .query_map(params![id], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(track_ids, vec![1, 2, 3, 4]);
    }

    #[test]
    fn create_with_tracks_creates_playlist_and_appends_in_one_call() {
        let mut conn = seeded_conn();
        let id = create_with_tracks(&mut conn, "Mix", &[1, 2, 3]).unwrap();

        let name: String = conn
            .query_row(
                "SELECT name FROM playlists WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(name, "Mix");

        let track_ids: Vec<i64> = conn
            .prepare(
                "SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
            )
            .unwrap()
            .query_map(params![id], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(track_ids, vec![1, 2, 3]);
    }

    #[test]
    fn create_with_tracks_empty_slice_creates_empty_playlist() {
        let mut conn = seeded_conn();
        let id = create_with_tracks(&mut conn, "Empty", &[]).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM playlist_tracks WHERE playlist_id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    /// TDD regression for the import non-atomicity finding: if the append
    /// step fails partway (here, a `track_id` that doesn't exist, tripping
    /// the `playlist_tracks.track_id` foreign key), the whole transaction
    /// rolls back — no orphaned empty playlist row is left in `playlists`.
    #[test]
    fn create_with_tracks_rolls_back_playlist_row_on_fk_violation() {
        let mut conn = seeded_conn();
        let before: i64 = conn
            .query_row("SELECT COUNT(*) FROM playlists", [], |r| r.get(0))
            .unwrap();

        // Track id 9999 doesn't exist in the seeded data (only 1..=5) — the
        // second insert should trip the foreign key and roll back the first.
        let result = create_with_tracks(&mut conn, "Bad Playlist", &[1, 9999]);
        assert!(result.is_err());

        let after: i64 = conn
            .query_row("SELECT COUNT(*) FROM playlists", [], |r| r.get(0))
            .unwrap();
        assert_eq!(before, after, "no playlist row should survive the rollback");

        let name_exists: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM playlists WHERE name = 'Bad Playlist'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(name_exists, 0);
    }

    #[test]
    fn remove_positions_single() {
        let mut conn = seeded_conn();
        let id = create(&conn, "P1").unwrap();
        add_tracks(&mut conn, id, &[1, 2, 3, 4]).unwrap();
        let removed = remove_positions(&mut conn, id, &[1]).unwrap();
        assert_eq!(removed, 1);

        let track_ids: Vec<i64> = conn
            .prepare(
                "SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
            )
            .unwrap()
            .query_map(params![id], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(track_ids, vec![1, 3, 4]);

        let positions: Vec<i64> = conn
            .prepare(
                "SELECT position FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
            )
            .unwrap()
            .query_map(params![id], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(positions, vec![0, 1, 2]); // Renumbered to be contiguous
    }

    #[test]
    fn remove_positions_multiple() {
        let mut conn = seeded_conn();
        let id = create(&conn, "P1").unwrap();
        add_tracks(&mut conn, id, &[1, 2, 3, 4, 5]).unwrap();
        let removed = remove_positions(&mut conn, id, &[1, 3]).unwrap();
        assert_eq!(removed, 2);

        let track_ids: Vec<i64> = conn
            .prepare(
                "SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
            )
            .unwrap()
            .query_map(params![id], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(track_ids, vec![1, 3, 5]);

        let positions: Vec<i64> = conn
            .prepare(
                "SELECT position FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
            )
            .unwrap()
            .query_map(params![id], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(positions, vec![0, 1, 2]);
    }

    #[test]
    fn remove_positions_empty_slice() {
        let mut conn = seeded_conn();
        let id = create(&conn, "P1").unwrap();
        add_tracks(&mut conn, id, &[1, 2, 3]).unwrap();
        let removed = remove_positions(&mut conn, id, &[]).unwrap();
        assert_eq!(removed, 0);

        let track_ids: Vec<i64> = conn
            .prepare(
                "SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
            )
            .unwrap()
            .query_map(params![id], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(track_ids, vec![1, 2, 3]);
    }

    #[test]
    fn move_position_down() {
        let mut conn = seeded_conn();
        let id = create(&conn, "P1").unwrap();
        add_tracks(&mut conn, id, &[1, 2, 3, 4]).unwrap();
        move_position(&mut conn, id, 0, 2).unwrap();

        let track_ids: Vec<i64> = conn
            .prepare(
                "SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
            )
            .unwrap()
            .query_map(params![id], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(track_ids, vec![2, 3, 1, 4]);
    }

    #[test]
    fn move_position_up() {
        let mut conn = seeded_conn();
        let id = create(&conn, "P1").unwrap();
        add_tracks(&mut conn, id, &[1, 2, 3, 4]).unwrap();
        move_position(&mut conn, id, 3, 1).unwrap();

        let track_ids: Vec<i64> = conn
            .prepare(
                "SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
            )
            .unwrap()
            .query_map(params![id], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(track_ids, vec![1, 4, 2, 3]);
    }

    #[test]
    fn move_position_same_is_noop() {
        let mut conn = seeded_conn();
        let id = create(&conn, "P1").unwrap();
        add_tracks(&mut conn, id, &[1, 2, 3]).unwrap();
        move_position(&mut conn, id, 1, 1).unwrap();

        let track_ids: Vec<i64> = conn
            .prepare(
                "SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
            )
            .unwrap()
            .query_map(params![id], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(track_ids, vec![1, 2, 3]);
    }

    #[test]
    fn smart_rules_to_sql_empty_rules() {
        let (where_clause, params) = smart_rules_to_sql("[]").unwrap();
        assert_eq!(where_clause, "1=1");
        assert!(params.is_empty());
    }

    #[test]
    fn smart_rules_to_sql_equals() {
        let json = r#"[{"field":"artist","op":"=","value":"Alice"}]"#;
        let (where_clause, params) = smart_rules_to_sql(json).unwrap();
        assert!(where_clause.contains("artist = ?"));
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn smart_rules_to_sql_not_null() {
        let json = r#"[{"field":"last_played_at","op":"not-null"}]"#;
        let (where_clause, params) = smart_rules_to_sql(json).unwrap();
        assert!(where_clause.contains("last_played_at IS NOT NULL"));
        assert!(params.is_empty());
    }

    #[test]
    fn smart_rules_to_sql_is_null() {
        let json = r#"[{"field":"rating","op":"is-null"}]"#;
        let (where_clause, params) = smart_rules_to_sql(json).unwrap();
        assert!(where_clause.contains("rating IS NULL"));
        assert!(params.is_empty());
    }

    #[test]
    fn smart_rules_to_sql_contains_with_escaping() {
        let json = r#"[{"field":"title","op":"contains","value":"50%"}]"#;
        let (_where_clause, params) = smart_rules_to_sql(json).unwrap();
        assert_eq!(params.len(), 1);
        // Verify % is escaped
        if let rusqlite::types::Value::Text(s) = &params[0] {
            assert!(s.contains("\\%"));
        }
    }

    #[test]
    fn smart_rules_to_sql_contains_underscore_escaping() {
        let json = r#"[{"field":"artist","op":"contains","value":"X_Y"}]"#;
        let (_where_clause, params) = smart_rules_to_sql(json).unwrap();
        if let rusqlite::types::Value::Text(s) = &params[0] {
            assert!(s.contains("\\_"));
        }
    }

    #[test]
    fn smart_rules_to_sql_contains_backslash_escaping() {
        // Test that a user value with backslash is fully escaped (no live wildcards).
        // Input: "a\\%" in JSON → represents string value a\% (one backslash, one percent)
        // After escaping: backslash → \\, percent → \%, result: a\\\%
        // After wrapping wildcards: %a\\\%%
        // In Rust source string literal: "%a\\\\\\%%"
        let json = r#"[{"field":"title","op":"contains","value":"a\\%"}]"#;
        let (_where_clause, params) = smart_rules_to_sql(json).unwrap();
        let expected = "%a\\\\\\%%";
        let rusqlite::types::Value::Text(s) = &params[0] else {
            panic!("expected text param")
        };
        assert_eq!(s, expected);
    }

    #[test]
    fn smart_rules_to_sql_contains_non_string_value_error() {
        // Contains operator with numeric value should error, not degrade to %%
        let json = r#"[{"field":"title","op":"contains","value":42}]"#;
        let result = smart_rules_to_sql(json);
        assert!(matches!(result, Err(SmartRulesError::InvalidValue(_))));
    }

    #[test]
    fn smart_rules_to_sql_missing_value_on_equals() {
        let json = r#"[{"field":"title","op":"="}]"#;
        let result = smart_rules_to_sql(json);
        assert!(matches!(result, Err(SmartRulesError::MissingValue(_))));
    }

    #[test]
    fn smart_rules_to_sql_missing_value_on_not_equals() {
        let json = r#"[{"field":"artist","op":"!="}]"#;
        let result = smart_rules_to_sql(json);
        assert!(matches!(result, Err(SmartRulesError::MissingValue(_))));
    }

    #[test]
    fn smart_rules_to_sql_missing_value_on_gte() {
        let json = r#"[{"field":"rating","op":">="}]"#;
        let result = smart_rules_to_sql(json);
        assert!(matches!(result, Err(SmartRulesError::MissingValue(_))));
    }

    #[test]
    fn smart_rules_to_sql_missing_value_on_lte() {
        let json = r#"[{"field":"rating","op":"<="}]"#;
        let result = smart_rules_to_sql(json);
        assert!(matches!(result, Err(SmartRulesError::MissingValue(_))));
    }

    #[test]
    fn smart_rules_to_sql_missing_value_on_gt() {
        let json = r#"[{"field":"duration_ms","op":">"}]"#;
        let result = smart_rules_to_sql(json);
        assert!(matches!(result, Err(SmartRulesError::MissingValue(_))));
    }

    #[test]
    fn smart_rules_to_sql_missing_value_on_lt() {
        let json = r#"[{"field":"duration_ms","op":"<"}]"#;
        let result = smart_rules_to_sql(json);
        assert!(matches!(result, Err(SmartRulesError::MissingValue(_))));
    }

    #[test]
    fn smart_rules_to_sql_missing_value_on_contains() {
        let json = r#"[{"field":"title","op":"contains"}]"#;
        let result = smart_rules_to_sql(json);
        assert!(matches!(result, Err(SmartRulesError::MissingValue(_))));
    }

    #[test]
    fn move_position_out_of_range_unchanged() {
        let mut conn = seeded_conn();
        let id = create(&conn, "P1").unwrap();
        add_tracks(&mut conn, id, &[1, 2, 3]).unwrap();

        // Get initial track order
        let initial_tracks: Vec<i64> = conn
            .prepare(
                "SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
            )
            .unwrap()
            .query_map(params![id], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        // Move out of range
        let result = move_position(&mut conn, id, 10, 1);
        assert!(result.is_ok());

        // Verify tracks are unchanged
        let final_tracks: Vec<i64> = conn
            .prepare(
                "SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
            )
            .unwrap()
            .query_map(params![id], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(initial_tracks, final_tracks);
    }

    #[test]
    fn smart_rules_to_sql_unknown_field_error() {
        let json = r#"[{"field":"title; DROP TABLE tracks--","op":"=","value":"x"}]"#;
        let result = smart_rules_to_sql(json);
        assert!(matches!(result, Err(SmartRulesError::UnknownField(_))));
    }

    #[test]
    fn smart_rules_to_sql_unknown_op_error() {
        let json = r#"[{"field":"artist","op":"unknown_op","value":"x"}]"#;
        let result = smart_rules_to_sql(json);
        assert!(matches!(result, Err(SmartRulesError::UnknownOperator(_))));
    }

    #[test]
    fn smart_rules_to_sql_and_joined() {
        let json =
            r#"[{"field":"rating","op":">=","value":4},{"field":"artist","op":"=","value":"Bob"}]"#;
        let (where_clause, params) = smart_rules_to_sql(json).unwrap();
        assert!(where_clause.contains("AND"));
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn list_smart_returns_seeded_playlists() {
        let conn = seeded_conn();
        let playlists = list_smart(&conn).unwrap();
        assert_eq!(playlists.len(), 3);

        // Check by name
        let names: Vec<&str> = playlists.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"Recently played"));
        assert!(names.contains(&"Top rated"));
        assert!(names.contains(&"Recently added"));
    }

    #[test]
    fn list_smart_recently_played_seed() {
        let conn = seeded_conn();
        let playlists = list_smart(&conn).unwrap();
        let recently_played = playlists
            .iter()
            .find(|p| p.name == "Recently played")
            .unwrap();
        assert_eq!(
            recently_played.rules_json,
            r#"[{"field":"last_played_at","op":"not-null"}]"#
        );
        assert_eq!(recently_played.sort_field, "last_played_at");
        assert_eq!(recently_played.sort_dir, "desc");
        assert_eq!(recently_played.limit_count, Some(50));
    }

    #[test]
    fn list_smart_top_rated_seed() {
        let conn = seeded_conn();
        let playlists = list_smart(&conn).unwrap();
        let top_rated = playlists.iter().find(|p| p.name == "Top rated").unwrap();
        assert_eq!(
            top_rated.rules_json,
            r#"[{"field":"rating","op":">=","value":4}]"#
        );
        assert_eq!(top_rated.sort_field, "rating");
        assert_eq!(top_rated.sort_dir, "desc");
        assert_eq!(top_rated.limit_count, None);
    }

    #[test]
    fn list_smart_recently_added_seed() {
        let conn = seeded_conn();
        let playlists = list_smart(&conn).unwrap();
        let recently_added = playlists
            .iter()
            .find(|p| p.name == "Recently added")
            .unwrap();
        assert_eq!(recently_added.rules_json, "[]");
        assert_eq!(recently_added.sort_field, "added_at");
        assert_eq!(recently_added.sort_dir, "desc");
        assert_eq!(recently_added.limit_count, Some(50));
    }
}
