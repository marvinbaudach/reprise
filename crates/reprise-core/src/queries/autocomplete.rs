//! Autocomplete suggestions for the tag editor: distinct column values
//! from the library, ranked by track count, matching prefix or substring
//! (TAG-6), plus the single best prefix match for inline ghost completion
//! (TAG-7).
//!
//! ## Ranking: prefix hits before substring hits
//!
//! TAG-6 requires prefix matches to always outrank substring-only matches,
//! regardless of track count — `"Cog"` typed against a library containing
//! both "Cognac" (a substring hit only if input were e.g. "gn") and
//! "Cogitations" must show the prefix hit first even if the substring match
//! has far more tracks. The `ORDER BY` therefore sorts on a `CASE WHEN …
//! LIKE '<input>%'` bucket (0 = prefix, 1 = substring-only) before track
//! count, so count only breaks ties *within* the same bucket.

use rusqlite::{Connection, OptionalExtension};

/// Row cap for the autocomplete dropdown (TAG-6).
pub const MAX_SUGGESTIONS: usize = 6;

/// Minimum characters typed before the dropdown appears (TAG-6). The inline
/// ghost (TAG-7) has no such floor — it can show below this length.
pub const MIN_DROPDOWN_CHARS: usize = 2;

use super::clauses::PRESENT;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutocompleteColumn {
    Artist,
    Album,
    AlbumArtist,
    Genre,
}

impl AutocompleteColumn {
    fn sql_column(self) -> &'static str {
        match self {
            Self::Artist => "artist",
            Self::Album => "album",
            Self::AlbumArtist => "album_artist",
            Self::Genre => "genre",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutocompleteSuggestion {
    pub value: String,
    pub track_count: i64,
}

/// Escapes SQL `LIKE` metacharacters (`\`, `%`, `_`) in `input`, so a literal
/// value typed by the user is never (mis)read as a pattern.
fn escape_like(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// Returns distinct non-empty values for `column`, case-insensitively
/// matching `input` as prefix or substring. Prefix hits always rank before
/// substring-only hits (TAG-6); within a bucket, higher track count first,
/// then case-insensitive alphabetical. Limited to `limit` results.
pub fn query_autocomplete_suggestions(
    conn: &Connection,
    column: AutocompleteColumn,
    input: &str,
    limit: usize,
) -> Result<Vec<AutocompleteSuggestion>, rusqlite::Error> {
    let col = column.sql_column();
    let sql = format!(
        "SELECT {col}, COUNT(*) AS cnt \
         FROM tracks \
         WHERE {PRESENT} AND {col} != '' AND {col} LIKE ?1 ESCAPE '\\' \
         GROUP BY {col} \
         ORDER BY (CASE WHEN {col} LIKE ?2 ESCAPE '\\' THEN 0 ELSE 1 END) ASC, \
                  cnt DESC, {col} COLLATE NOCASE ASC \
         LIMIT ?3"
    );
    let escaped = escape_like(input);
    let substring_pattern = format!("%{escaped}%");
    let prefix_pattern = format!("{escaped}%");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        rusqlite::params![substring_pattern, prefix_pattern, limit as i64],
        |row| {
            Ok(AutocompleteSuggestion {
                value: row.get(0)?,
                track_count: row.get(1)?,
            })
        },
    )?;
    rows.collect()
}

/// The single best prefix match for `input` (TAG-7's inline ghost) — the
/// same ranking as `query_autocomplete_suggestions`'s first row would be
/// *if* it were restricted to prefix matches only: unlike the dropdown, the
/// ghost never surfaces a substring-only match (typing "ide" must not ghost
/// "Suicide" — that's not a completion of what's typed). Returns `None` on
/// empty input, no prefix match, or a query error (logged, not propagated —
/// callers treat "no ghost" as the safe default rather than surfacing a
/// query failure to the editing UI).
pub fn query_ghost_completion(
    conn: &Connection,
    column: AutocompleteColumn,
    input: &str,
) -> Option<String> {
    if input.is_empty() {
        return None;
    }
    let col = column.sql_column();
    let sql = format!(
        "SELECT {col}, COUNT(*) AS cnt \
         FROM tracks \
         WHERE {PRESENT} AND {col} != '' AND {col} LIKE ?1 ESCAPE '\\' \
         GROUP BY {col} \
         ORDER BY cnt DESC, {col} COLLATE NOCASE ASC \
         LIMIT 1"
    );
    let pattern = format!("{}%", escape_like(input));
    let result = conn
        .query_row(&sql, rusqlite::params![pattern], |row| {
            row.get::<_, String>(0)
        })
        .optional();
    match result {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(%error, "autocomplete: ghost completion query failed");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded_db() -> Connection {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        for (id, artist, album, album_artist, genre) in [
            (1, "Cogitations", "Relinquished", "Cogitations", "Ambient"),
            (2, "Cogitations", "Relinquished", "Cogitations", "Ambient"),
            (
                3,
                "Cognitive Dissonance",
                "Fractures",
                "Cognitive Dissonance",
                "Post-Rock",
            ),
            (4, "Radio Cognac", "Midnight", "Radio Cognac", "Jazz"),
            (5, "Unrelated", "Other", "Unrelated", "Ambient"),
        ] {
            conn.execute(
                "INSERT INTO tracks (id,path,title,artist,album,album_artist,genre,added_at) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7,0)",
                rusqlite::params![
                    id,
                    format!("/music/{id}.flac"),
                    format!("Track {id}"),
                    artist,
                    album,
                    album_artist,
                    genre,
                ],
            )
            .unwrap();
        }
        conn
    }

    #[test]
    fn prefix_match_returns_results_sorted_by_track_count() {
        let conn = seeded_db();
        let results =
            query_autocomplete_suggestions(&conn, AutocompleteColumn::Artist, "Cog", 8).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].value, "Cogitations");
        assert_eq!(results[0].track_count, 2);
        assert_eq!(results[1].value, "Cognitive Dissonance");
        assert_eq!(results[1].track_count, 1);
        assert_eq!(results[2].value, "Radio Cognac");
        assert_eq!(results[2].track_count, 1);
    }

    #[test]
    fn substring_match_finds_non_prefix_hits() {
        let conn = seeded_db();
        let results =
            query_autocomplete_suggestions(&conn, AutocompleteColumn::Artist, "ognac", 8).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].value, "Radio Cognac");
    }

    #[test]
    fn case_insensitive_matching() {
        let conn = seeded_db();
        let results =
            query_autocomplete_suggestions(&conn, AutocompleteColumn::Artist, "cog", 8).unwrap();
        assert!(
            results.len() >= 2,
            "case-insensitive match should find Cog*"
        );
    }

    #[test]
    fn limit_is_respected() {
        let conn = seeded_db();
        let results =
            query_autocomplete_suggestions(&conn, AutocompleteColumn::Artist, "Co", 1).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn empty_input_returns_all_distinct_values() {
        let conn = seeded_db();
        let results =
            query_autocomplete_suggestions(&conn, AutocompleteColumn::Genre, "", 8).unwrap();
        // Empty input matches all values with pattern "%%" - all rows match
        // Genres: Ambient (rows 1,2,5), Post-Rock (row 3), Jazz (row 4)
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].value, "Ambient");
        assert_eq!(results[0].track_count, 3);
    }

    #[test]
    fn missing_tracks_are_excluded() {
        let conn = seeded_db();
        conn.execute(
            "UPDATE tracks SET missing_since = 1, missing_reason = 'unknown' WHERE id = 1",
            [],
        )
        .unwrap();
        let results =
            query_autocomplete_suggestions(&conn, AutocompleteColumn::Artist, "Cog", 8).unwrap();
        let cog = results.iter().find(|s| s.value == "Cogitations").unwrap();
        assert_eq!(cog.track_count, 1, "missing track should not be counted");
    }

    #[test]
    fn empty_values_are_excluded() {
        let conn = seeded_db();
        conn.execute(
            "INSERT INTO tracks (id,path,title,artist,album,genre,added_at) \
             VALUES (99,'/x.flac','X','','','',0)",
            [],
        )
        .unwrap();
        let results =
            query_autocomplete_suggestions(&conn, AutocompleteColumn::Artist, "", 8).unwrap();
        assert!(
            results.iter().all(|s| !s.value.is_empty()),
            "empty-string values must not appear"
        );
    }

    #[test]
    fn album_artist_column_works() {
        let conn = seeded_db();
        let results =
            query_autocomplete_suggestions(&conn, AutocompleteColumn::AlbumArtist, "Cog", 8)
                .unwrap();
        assert!(!results.is_empty());
    }

    fn insert_artist(conn: &Connection, id: i64, artist: &str) {
        conn.execute(
            "INSERT INTO tracks (id,path,title,artist,album,genre,added_at) \
             VALUES (?1,?2,?3,?4,'Album','Genre',0)",
            rusqlite::params![
                id,
                format!("/music/rank-{id}.flac"),
                format!("Track {id}"),
                artist
            ],
        )
        .unwrap();
    }

    #[test]
    fn tag_6_prefix_ranks_before_substring() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        // "Zzz Cognac Band" only contains "ogn" as a substring, but has far
        // more tracks than "Ognomatic", which is a true prefix match.
        for id in 1..=5 {
            insert_artist(&conn, id, "Zzz Cognac Band");
        }
        insert_artist(&conn, 6, "Ognomatic");

        let results =
            query_autocomplete_suggestions(&conn, AutocompleteColumn::Artist, "ogn", 8).unwrap();
        assert_eq!(
            results[0].value, "Ognomatic",
            "a prefix match must rank first even with fewer tracks"
        );
        assert_eq!(results[1].value, "Zzz Cognac Band");
    }

    #[test]
    fn tag_6_limit_is_six() {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        for id in 1..=8 {
            insert_artist(&conn, id, &format!("Genre Artist {id}"));
        }
        let results =
            query_autocomplete_suggestions(&conn, AutocompleteColumn::Artist, "Genre", 8).unwrap();
        assert_eq!(results.len(), 8, "sanity: 8 distinct matches exist");

        let capped = query_autocomplete_suggestions(
            &conn,
            AutocompleteColumn::Artist,
            "Genre",
            MAX_SUGGESTIONS,
        )
        .unwrap();
        assert_eq!(capped.len(), MAX_SUGGESTIONS);
        assert_eq!(MAX_SUGGESTIONS, 6);
    }

    #[test]
    fn tag_7_ghost_is_best_prefix_by_track_count() {
        let conn = seeded_db();
        let ghost = query_ghost_completion(&conn, AutocompleteColumn::Artist, "Cog");
        assert_eq!(ghost, Some("Cogitations".into()));
    }

    #[test]
    fn tag_7_ghost_none_without_prefix_match() {
        let conn = seeded_db();
        // "Radio Cognac" only matches "ognac" as a substring, never a prefix.
        let ghost = query_ghost_completion(&conn, AutocompleteColumn::Artist, "ognac");
        assert_eq!(ghost, None);
    }

    #[test]
    fn ghost_completion_is_none_for_empty_input() {
        let conn = seeded_db();
        assert_eq!(
            query_ghost_completion(&conn, AutocompleteColumn::Artist, ""),
            None
        );
    }

    #[test]
    fn ghost_completion_is_none_without_any_match() {
        let conn = seeded_db();
        assert_eq!(
            query_ghost_completion(&conn, AutocompleteColumn::Artist, "Zzzzz"),
            None
        );
    }

    #[test]
    fn min_dropdown_chars_constant_is_two() {
        assert_eq!(MIN_DROPDOWN_CHARS, 2);
    }
}
