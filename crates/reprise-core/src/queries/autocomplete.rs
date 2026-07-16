//! Autocomplete suggestions for the tag editor: distinct column values
//! from the library, ranked by track count, matching prefix or substring.

use rusqlite::Connection;

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

/// Returns distinct non-empty values for `column`, case-insensitively
/// matching `input` as prefix or substring, sorted by track count
/// descending, limited to `limit` results.
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
         WHERE missing = 0 AND {col} != '' AND {col} LIKE ?1 ESCAPE '\\' \
         GROUP BY {col} \
         ORDER BY cnt DESC, {col} COLLATE NOCASE ASC \
         LIMIT ?2"
    );
    let pattern = format!(
        "%{}%",
        input
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params![pattern, limit as i64], |row| {
        Ok(AutocompleteSuggestion {
            value: row.get(0)?,
            track_count: row.get(1)?,
        })
    })?;
    rows.collect()
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
        conn.execute("UPDATE tracks SET missing = 1 WHERE id = 1", [])
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
}
