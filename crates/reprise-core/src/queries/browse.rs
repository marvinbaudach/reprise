//! Exact, bound Genre/Artist/Album facets for the flat Library source.

use rusqlite::Connection;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BrowseFilter {
    pub genre: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
}

impl BrowseFilter {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.genre.is_none() && self.artist.is_none() && self.album.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowseFacet {
    Genre,
    Artist,
    Album,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowseValue {
    pub value: String,
    pub count: i64,
}

pub fn query_browse_values(
    conn: &Connection,
    facet: BrowseFacet,
    filter: &BrowseFilter,
) -> Result<Vec<BrowseValue>, rusqlite::Error> {
    let (column, effective_filter) = match facet {
        BrowseFacet::Genre => ("genre", BrowseFilter::default()),
        BrowseFacet::Artist => (
            "artist",
            BrowseFilter {
                genre: filter.genre.clone(),
                ..BrowseFilter::default()
            },
        ),
        BrowseFacet::Album => (
            "album",
            BrowseFilter {
                genre: filter.genre.clone(),
                artist: filter.artist.clone(),
                album: None,
            },
        ),
    };
    let (clause, values) = browse_clause(&effective_filter, 1);
    let sql = format!(
        "SELECT {column}, count(*) FROM tracks \
         WHERE missing = 0{clause} \
         GROUP BY {column} ORDER BY {column} COLLATE NOCASE ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(values), |row| {
        Ok(BrowseValue {
            value: row.get(0)?,
            count: row.get(1)?,
        })
    })?;
    rows.collect()
}

pub(super) fn browse_clause(filter: &BrowseFilter, first_param: usize) -> (String, Vec<String>) {
    let mut clause = String::new();
    let mut values = Vec::new();
    for (column, value) in [
        ("genre", &filter.genre),
        ("artist", &filter.artist),
        ("album", &filter.album),
    ] {
        let Some(value) = value else {
            continue;
        };
        let parameter = first_param + values.len();
        clause.push_str(&format!(" AND {column} = ?{parameter}"));
        values.push(value.clone());
    }
    (clause, values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_browse_filter_has_no_clause_or_values() {
        let filter = BrowseFilter::default();
        assert!(filter.is_empty());
        assert_eq!(browse_clause(&filter, 4), (String::new(), Vec::new()));
    }

    #[test]
    fn browse_clause_numbers_only_present_fields_in_canonical_order() {
        let filter = BrowseFilter {
            genre: Some("Rock".into()),
            artist: None,
            album: Some("Live".into()),
        };
        assert!(!filter.is_empty());
        assert_eq!(
            browse_clause(&filter, 4),
            (
                " AND genre = ?4 AND album = ?5".into(),
                vec!["Rock".into(), "Live".into()],
            )
        );
    }

    #[test]
    fn hostile_facet_value_never_appears_in_sql_text() {
        let hostile = "' OR 1=1 --";
        let filter = BrowseFilter {
            artist: Some(hostile.into()),
            ..BrowseFilter::default()
        };
        let (sql, values) = browse_clause(&filter, 1);
        assert_eq!(sql, " AND artist = ?1");
        assert_eq!(values, vec![hostile]);
        assert!(!sql.contains(hostile));
    }

    fn seeded_facets() -> Connection {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        for (id, artist, album, genre) in [
            (1, "A", "Stage", "Rock"),
            (2, "A", "Stage", "Rock"),
            (3, "B", "Other", "Rock"),
            (4, "A", "Blue", "Jazz"),
            (5, "", "", ""),
        ] {
            conn.execute(
                "INSERT INTO tracks (id,path,title,artist,album,genre,added_at) \
                 VALUES (?1,?2,?3,?4,?5,?6,0)",
                rusqlite::params![
                    id,
                    format!("/x/{id}.flac"),
                    format!("T{id}"),
                    artist,
                    album,
                    genre
                ],
            )
            .unwrap();
        }
        conn
    }

    #[test]
    fn artist_values_are_constrained_by_genre() {
        let conn = seeded_facets();
        let filter = BrowseFilter {
            genre: Some("Rock".into()),
            ..BrowseFilter::default()
        };
        assert_eq!(
            query_browse_values(&conn, BrowseFacet::Artist, &filter).unwrap(),
            vec![
                BrowseValue {
                    value: "A".into(),
                    count: 2
                },
                BrowseValue {
                    value: "B".into(),
                    count: 1
                },
            ]
        );
    }

    #[test]
    fn album_values_are_constrained_by_genre_and_artist() {
        let conn = seeded_facets();
        let filter = BrowseFilter {
            genre: Some("Rock".into()),
            artist: Some("A".into()),
            album: Some("ignored-current-album".into()),
        };
        assert_eq!(
            query_browse_values(&conn, BrowseFacet::Album, &filter).unwrap(),
            vec![BrowseValue {
                value: "Stage".into(),
                count: 2
            }]
        );
    }

    #[test]
    fn empty_metadata_is_returned_as_typed_empty_value() {
        let conn = seeded_facets();
        assert!(
            query_browse_values(&conn, BrowseFacet::Genre, &BrowseFilter::default())
                .unwrap()
                .contains(&BrowseValue {
                    value: String::new(),
                    count: 1
                })
        );
    }
}
