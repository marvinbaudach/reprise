//! Manual and smart playlist management.
//!
//! Manual playlists store ordered track references (duplicates allowed, like
//! Rhythmbox). Smart playlists filter tracks via a rules JSON document with
//! sort and limit options. Both types maintain gapless 0-indexed positions
//! across all operations (create, add, remove, move).

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[path = "playlists_api.rs"]
mod api;
pub use super::playlist_delete::delete;
pub use api::*;

pub const RECENTLY_ADDED_NAME: &str = "Recently added";
pub const RECENTLY_ADDED_ROLE: &str = "recently_added";

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
    pub role: Option<String>,
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
fn create_in(conn: &Connection, name: &str) -> Result<i64, rusqlite::Error> {
    crate::events::in_txn(conn, |conn| {
        let id = create_playlist_row(conn, name)?;
        crate::events::record(conn, "playlist", &id.to_string(), "create")?;
        Ok(id)
    })
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

    // Stage-3 close-out fix: this used to be `.unwrap_or(-1)`, silently
    // turning a transient DB error (lock contention, I/O failure) into
    // "no rows yet" — the new playlist would land at position 0 (or
    // collide/reorder against an existing position 0) instead of the
    // caller ever finding out the query failed. `create_playlist_row`
    // already returns `Result`, so propagate via `?` like every other
    // fallible step in this function.
    let max_position: i64 = conn.query_row(
        "SELECT COALESCE(MAX(position), -1) FROM playlists",
        [],
        |r| r.get(0),
    )?;
    let new_position = max_position + 1;

    conn.execute(
        "INSERT INTO playlists (name, position) VALUES (?1, ?2)",
        params![insert_name, new_position],
    )?;

    let id = conn.last_insert_rowid();
    Ok(id)
}

/// Renames a playlist by id, returning the number of rows changed — `0` when no
/// playlist has that id. A no-op rename records **no** `change_log` event, so a
/// stale or absent id can never fabricate a phantom "rename" change (the
/// event-without-change bug class); this lets a caller drop any pre-check
/// TOCTOU workaround and simply branch on the returned count.
#[allow(dead_code)]
fn rename_in(conn: &Connection, id: i64, name: &str) -> Result<usize, rusqlite::Error> {
    crate::events::in_txn(conn, |conn| {
        let changed = conn.execute(
            "UPDATE playlists SET name = ?1 WHERE id = ?2",
            params![name, id],
        )?;
        // Only a real change is logged — mirrors the dedup posture of
        // create_smart / set_setting / the stale-delete no-op.
        if changed > 0 {
            crate::events::record(conn, "playlist", &id.to_string(), "rename")?;
        }
        Ok(changed)
    })
}

/// Lists all *user* manual playlists, ordered by position (ascending).
/// Includes track count for each (0 if empty). System playlists carrying a
/// `role` (schema v27 — e.g. the conversion drop playlist) are excluded: they
/// are surfaced by their own role-specific view, never in the ordinary
/// playlist list. Pre-v27 rows all have `role IS NULL`, so this filter is a
/// no-op for every existing playlist.
fn list_in(conn: &Connection) -> Result<Vec<PlaylistSummary>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.name, COALESCE(COUNT(pt.track_id), 0) as track_count \
         FROM playlists p \
         LEFT JOIN playlist_tracks pt ON p.id = pt.playlist_id \
         WHERE p.role IS NULL \
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

/// Looks up one manual playlist by id, or `Ok(None)` when it does not exist —
/// the single-row companion to [`list`]. A caller that needs just one
/// playlist's summary (a CLI header, a delete's expected-name lookup) uses this
/// instead of scanning and filtering the whole `list`. `track_count` is
/// computed exactly as in `list`, so the two always agree.
fn get_in(conn: &Connection, id: i64) -> Result<Option<PlaylistSummary>, rusqlite::Error> {
    conn.query_row(
        "SELECT p.id, p.name, COALESCE(COUNT(pt.track_id), 0) as track_count \
         FROM playlists p \
         LEFT JOIN playlist_tracks pt ON p.id = pt.playlist_id \
         WHERE p.id = ?1 \
         GROUP BY p.id",
        params![id],
        |row| {
            Ok(PlaylistSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                track_count: row.get(2)?,
            })
        },
    )
    .optional()
}

/// The playlist's track ids in stored (`position`) order. Empty for an empty
/// or non-existent playlist — callers treat "no playable tracks" as invalid
/// input at their boundary.
fn track_ids_in(conn: &Connection, playlist_id: i64) -> Result<Vec<i64>, rusqlite::Error> {
    let mut stmt = conn
        .prepare("SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")?;
    let ids = stmt
        .query_map(params![playlist_id], |row| row.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids)
}

/// Appends tracks to a playlist at the end (appends to the highest position).
/// Positions are contiguous. Duplicates allowed (Rhythmbox behavior).
/// All inserts happen in one transaction.
#[allow(dead_code)]
fn add_tracks_in(
    conn: &Connection,
    playlist_id: i64,
    track_ids: &[i64],
) -> Result<u32, rusqlite::Error> {
    if track_ids.is_empty() {
        return Ok(0);
    }

    let tx = conn.unchecked_transaction()?;
    let inserted = append_tracks_rows(&tx, playlist_id, track_ids)?;
    if inserted > 0 {
        crate::events::record(&tx, "playlist", &playlist_id.to_string(), "add")?;
    }
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
    // See `create_playlist_row`'s matching comment: propagate a transient DB
    // error via `?` instead of silently treating it as "playlist is empty"
    // (`.unwrap_or(-1)` would have appended at position 0, potentially
    // colliding with/reordering an existing row).
    let max_position: i64 = conn.query_row(
        "SELECT COALESCE(MAX(position), -1) FROM playlist_tracks WHERE playlist_id = ?1",
        params![playlist_id],
        |r| r.get(0),
    )?;

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
/// must succeed or fail together. Callers: M3U import (`ui::playlist_io::
/// import_playlist`), the "New playlist…" track-list context-menu action
/// (`ui::track_actions::create_playlist_and_add` — adopted in Task 9's
/// review fold-in; it used to call `create` + `add_tracks` separately, which
/// could leave an orphaned empty playlist behind on a partial failure), and
/// the `REPRISE_SMOKE_SEED_PLAYLIST` headless dev hook (`main.rs`).
fn create_with_tracks_in_db(
    conn: &Connection,
    name: &str,
    track_ids: &[i64],
) -> Result<i64, rusqlite::Error> {
    let tx = conn.unchecked_transaction()?;
    let playlist_id = create_with_tracks_in(&tx, name, track_ids)?;
    crate::events::record(&tx, "playlist", &playlist_id.to_string(), "create")?;
    tx.commit()?;
    Ok(playlist_id)
}

pub(crate) fn create_with_tracks_in(
    conn: &Connection,
    name: &str,
    track_ids: &[i64],
) -> Result<i64, rusqlite::Error> {
    let playlist_id = create_playlist_row(conn, name)?;
    if !track_ids.is_empty() {
        append_tracks_rows(conn, playlist_id, track_ids)?;
    }
    Ok(playlist_id)
}

/// Finds the single playlist carrying `role`, or `None`. Roles are unique by
/// convention (there is at most one conversion playlist); the lowest id wins
/// if a database somehow holds two.
pub(crate) fn find_role_playlist_in(
    conn: &Connection,
    role: &str,
) -> Result<Option<i64>, rusqlite::Error> {
    conn.query_row(
        "SELECT id FROM playlists WHERE role = ?1 ORDER BY id LIMIT 1",
        params![role],
        |row| row.get(0),
    )
    .optional()
}

/// Gets, or creates, the system playlist carrying `role` — idempotent, so a
/// caller can ensure the conversion drop playlist exists on every startup or
/// first drop without piling up duplicates. Returns its id. A freshly created
/// role playlist logs one `create` change-log event; an existing one logs
/// nothing (mirrors `create_smart`'s dedup posture).
pub(crate) fn ensure_role_playlist_in(
    conn: &Connection,
    name: &str,
    role: &str,
) -> Result<i64, rusqlite::Error> {
    crate::events::in_txn(conn, |conn| {
        if let Some(id) = find_role_playlist_in(conn, role)? {
            return Ok(id);
        }
        let trimmed = name.trim();
        let insert_name = if trimmed.is_empty() { name } else { trimmed };
        let max_position: i64 = conn.query_row(
            "SELECT COALESCE(MAX(position), -1) FROM playlists",
            [],
            |row| row.get(0),
        )?;
        conn.execute(
            "INSERT INTO playlists (name, position, role) VALUES (?1, ?2, ?3)",
            params![insert_name, max_position + 1, role],
        )?;
        let id = conn.last_insert_rowid();
        crate::events::record(conn, "playlist", &id.to_string(), "create")?;
        Ok(id)
    })
}

/// The role of a playlist, or `None` when it is an ordinary user playlist (or
/// does not exist). Lets a frontend tell the conversion drop playlist apart
/// from user playlists without hardcoding an id.
fn playlist_role_in(conn: &Connection, id: i64) -> Result<Option<String>, rusqlite::Error> {
    conn.query_row(
        "SELECT role FROM playlists WHERE id = ?1",
        params![id],
        |row| row.get::<_, Option<String>>(0),
    )
    .optional()
    .map(Option::flatten)
}

/// Removes tracks at the specified positions and renumbers the remaining
/// tracks to keep positions contiguous (0..n-1). Multiple removes happen in
/// one transaction.
#[allow(dead_code)]
fn remove_positions_in(
    conn: &Connection,
    playlist_id: i64,
    positions: &[u32],
) -> Result<u32, rusqlite::Error> {
    if positions.is_empty() {
        return Ok(0);
    }

    let tx = conn.unchecked_transaction()?;

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

    renumber_positions(&tx, playlist_id)?;

    if deleted > 0 {
        crate::events::record(&tx, "playlist", &playlist_id.to_string(), "remove")?;
    }
    tx.commit()?;
    Ok(deleted)
}

/// Renumbers `playlist_id`'s `playlist_tracks.position` values to be
/// contiguous (0..n-1), preserving relative order — the gapless-position
/// invariant this module's doc comment promises. Extracted from
/// `remove_positions` (Stage-3 close-out) so `queries::remove_missing_
/// tracks` can reuse the exact same renumbering after a hard-delete's
/// `ON DELETE CASCADE` leaves gaps behind (see that function's doc comment
/// for the full incident this closes). Takes a plain `&Connection` (not
/// `&mut`) so it can run against either a bare `&Connection` or, via deref
/// coercion, a `&Transaction` — the same "shared logic, caller owns the
/// transaction" shape `create_playlist_row`/`append_tracks_rows` already
/// use. A no-op for a playlist with no rows (or one that's already gapless —
/// the `if old_pos != new_pos` guard skips a write for every row already in
/// place).
pub(crate) fn renumber_positions(
    conn: &Connection,
    playlist_id: i64,
) -> Result<(), rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT position FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position ASC",
    )?;
    let current_positions: Vec<i64> = stmt
        .query_map(params![playlist_id], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    for (new_pos, &old_pos) in current_positions.iter().enumerate() {
        if old_pos != new_pos as i64 {
            conn.execute(
                "UPDATE playlist_tracks SET position = ?1 WHERE playlist_id = ?2 AND position = ?3",
                params![new_pos as i64, playlist_id, old_pos],
            )?;
        }
    }
    Ok(())
}

/// Escapes `\`, `%`, `_` for use inside a SQL `LIKE ... ESCAPE '\'` pattern —
/// backslash first, so a literal backslash a user typed doesn't get
/// misread as introducing one of the wildcard escapes emitted next (Stage-3
/// close-out finding: the search filter's own `LIKE` sites in `queries.rs`
/// didn't escape `%`/`_` at all, so typing either in the search box silently
/// acted as a wildcard — inconsistent with this module's own `contains`
/// smart-rule operator, which already escaped correctly). Shared by both:
/// `contains` below, and every `filter_clause` LIKE-binding site in
/// `queries.rs`, so a literal `%`/`_` in a search box or a smart-rule value
/// is never misread as a wildcard, and the two can never drift apart on how
/// escaping works.
pub fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// Moves a track from one position to another, renumbering all affected
/// positions to stay contiguous (0..n-1). If `from` or `to` are out of range,
/// logs a warning and returns Ok (no-op). One transaction.
#[allow(dead_code)]
fn move_position_in(
    conn: &Connection,
    playlist_id: i64,
    from: u32,
    to: u32,
) -> Result<(), rusqlite::Error> {
    // See `create_playlist_row`'s matching comment: propagate a transient DB
    // error via `?` instead of silently treating it as "playlist is empty"
    // (`.unwrap_or(-1)` would have made every `from`/`to` look out of range,
    // silently no-op'ing a move instead of surfacing the real DB failure).
    let max_position: i64 = conn.query_row(
        "SELECT COALESCE(MAX(position), -1) FROM playlist_tracks WHERE playlist_id = ?1",
        params![playlist_id],
        |r| r.get(0),
    )?;

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

    let tx = conn.unchecked_transaction()?;

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

    crate::events::record(&tx, "playlist", &playlist_id.to_string(), "move")?;
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
                let escaped = escape_like(s);
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

/// Creates a smart playlist and returns its database id.
///
/// An identical playlist — same name, rules, sort and limit — is returned as
/// it stands instead of inserted twice. `smart_playlists.name` carries no
/// UNIQUE constraint, and a one-click entry point like the My Stats "Smart
/// Mix" CTA would otherwise pile up duplicates the user has to delete by hand.
/// Same name with different rules stays a distinct playlist.
fn create_smart_in(
    conn: &Connection,
    name: &str,
    rules_json: &str,
    sort_field: &str,
    sort_dir: &str,
    limit_count: Option<i64>,
) -> Result<i64, rusqlite::Error> {
    smart_rules_to_sql(rules_json)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    crate::events::in_txn(conn, |conn| {
        let existing = conn
            .query_row(
                "SELECT id FROM smart_playlists \
                 WHERE name = ?1 AND rules_json = ?2 AND sort_field = ?3 \
                   AND sort_dir = ?4 AND limit_count IS ?5 \
                 ORDER BY id LIMIT 1",
                params![name, rules_json, sort_field, sort_dir, limit_count],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        // A dedup hit persists nothing, so it must log nothing.
        if let Some(id) = existing {
            return Ok(id);
        }
        conn.execute(
            "INSERT INTO smart_playlists \
             (name, rules_json, sort_field, sort_dir, limit_count) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![name, rules_json, sort_field, sort_dir, limit_count],
        )?;
        let id = conn.last_insert_rowid();
        crate::events::record(conn, "smart_playlist", &id.to_string(), "create")?;
        Ok(id)
    })
}

/// Lists all smart playlists.
pub(crate) fn list_smart_in(conn: &Connection) -> Result<Vec<SmartPlaylist>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, name, rules_json, sort_field, sort_dir, limit_count, role
         FROM smart_playlists",
    )?;
    let smart_playlists = stmt.query_map([], |row| {
        Ok(SmartPlaylist {
            id: row.get(0)?,
            name: row.get(1)?,
            rules_json: row.get(2)?,
            sort_field: row.get(3)?,
            sort_dir: row.get(4)?,
            limit_count: row.get(5)?,
            role: row.get(6)?,
        })
    })?;

    let mut result = Vec::new();
    for playlist in smart_playlists {
        result.push(playlist?);
    }
    Ok(result)
}

#[cfg(test)]
#[path = "playlists_tests.rs"]
mod tests;
