//! Exact, bound Genre/Artist/Album/Year/Rating facets for the flat Library
//! source.
//!
//! Genre → Artist → Album form a cascade (a deeper facet is constrained by
//! the shallower ones). Year and Rating are standalone, additive constraints:
//! they narrow every query but do not participate in the cascade, so their
//! value lists reflect all *other* active facets.

use rusqlite::Connection;

use super::clauses::PRESENT;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BrowseFilter {
    pub genre: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    // Year and Rating are stored as strings so they reuse the shared value
    // chooser; SQLite's column affinity converts them back to integers when
    // they hit the `year`/`rating` columns. `serde(default)` keeps sessions
    // saved before these facets existed loading cleanly (they default to None).
    #[serde(default)]
    pub year: Option<String>,
    #[serde(default)]
    pub rating: Option<String>,
}

impl BrowseFilter {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.genre.is_none()
            && self.artist.is_none()
            && self.album.is_none()
            && self.year.is_none()
            && self.rating.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowseFacet {
    Genre,
    Artist,
    Album,
    Year,
    Rating,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowseValue {
    pub value: String,
    pub count: i64,
}

/// The SQL shape of a facet's distinct-value list: how to read the value out
/// as text, how to group and order it, and any predicate that drops rows
/// without a meaningful value.
struct FacetSql {
    /// Expression selected as the distinct value — always TEXT so the reader
    /// can map every facet uniformly, even the integer `year`/`rating`.
    select: &'static str,
    /// Column the rows are grouped by.
    group: &'static str,
    /// Ordering applied to the grouped rows.
    order: &'static str,
    /// Extra predicate appended to the `WHERE`, e.g. dropping NULL years.
    extra: &'static str,
}

fn facet_sql(facet: BrowseFacet) -> FacetSql {
    match facet {
        BrowseFacet::Genre => FacetSql {
            select: "genre",
            group: "genre",
            order: "genre COLLATE NOCASE ASC",
            extra: "",
        },
        BrowseFacet::Artist => FacetSql {
            select: "artist",
            group: "artist",
            order: "artist COLLATE NOCASE ASC",
            extra: "",
        },
        BrowseFacet::Album => FacetSql {
            select: "album",
            group: "album",
            order: "album COLLATE NOCASE ASC",
            extra: "",
        },
        // `year` is nullable; tracks without a tagged year are not a facet.
        BrowseFacet::Year => FacetSql {
            select: "CAST(year AS TEXT)",
            group: "year",
            order: "year ASC",
            extra: " AND year IS NOT NULL",
        },
        // `rating` is NOT NULL DEFAULT 0, so 0 (unrated) is a valid value.
        BrowseFacet::Rating => FacetSql {
            select: "CAST(rating AS TEXT)",
            group: "rating",
            order: "rating ASC",
            extra: "",
        },
    }
}

/// The filter that constrains a facet's own value list: the cascade parents
/// for Genre/Artist/Album, and every *other* active facet for the standalone
/// Year/Rating.
fn value_list_filter(facet: BrowseFacet, filter: &BrowseFilter) -> BrowseFilter {
    match facet {
        BrowseFacet::Genre => BrowseFilter::default(),
        BrowseFacet::Artist => BrowseFilter {
            genre: filter.genre.clone(),
            ..BrowseFilter::default()
        },
        BrowseFacet::Album => BrowseFilter {
            genre: filter.genre.clone(),
            artist: filter.artist.clone(),
            ..BrowseFilter::default()
        },
        BrowseFacet::Year => BrowseFilter {
            year: None,
            ..filter.clone()
        },
        BrowseFacet::Rating => BrowseFilter {
            rating: None,
            ..filter.clone()
        },
    }
}

pub fn query_browse_values(
    conn: &Connection,
    facet: BrowseFacet,
    filter: &BrowseFilter,
) -> Result<Vec<BrowseValue>, rusqlite::Error> {
    let FacetSql {
        select,
        group,
        order,
        extra,
    } = facet_sql(facet);
    let (clause, values) = browse_clause(&value_list_filter(facet, filter), 1);
    let sql = format!(
        "SELECT {select}, count(*) FROM tracks \
         WHERE {PRESENT}{extra}{clause} \
         GROUP BY {group} ORDER BY {order}"
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
        ("year", &filter.year),
        ("rating", &filter.rating),
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
            album: Some("Live".into()),
            ..BrowseFilter::default()
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
        // `year` is nullable and `rating` is NOT NULL DEFAULT 0; row 5 leaves
        // both at their "unknown" edge (NULL year, rating 0).
        for (id, artist, album, genre, year, rating) in [
            (1, "A", "Stage", "Rock", Some(2001), 5),
            (2, "A", "Stage", "Rock", Some(2001), 4),
            (3, "B", "Other", "Rock", Some(1999), 5),
            (4, "A", "Blue", "Jazz", Some(2001), 3),
            (5, "", "", "", None, 0),
        ] {
            conn.execute(
                "INSERT INTO tracks (id,path,title,artist,album,genre,year,rating,added_at) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,0)",
                rusqlite::params![
                    id,
                    format!("/x/{id}.flac"),
                    format!("T{id}"),
                    artist,
                    album,
                    genre,
                    year,
                    rating
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
            ..BrowseFilter::default()
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

    #[test]
    fn year_values_are_distinct_text_numbers_ordered_ascending_without_nulls() {
        let conn = seeded_facets();
        assert_eq!(
            query_browse_values(&conn, BrowseFacet::Year, &BrowseFilter::default()).unwrap(),
            vec![
                BrowseValue {
                    value: "1999".into(),
                    count: 1
                },
                BrowseValue {
                    value: "2001".into(),
                    count: 3
                },
            ]
        );
    }

    #[test]
    fn rating_values_include_zero_and_are_ordered_numerically() {
        let conn = seeded_facets();
        assert_eq!(
            query_browse_values(&conn, BrowseFacet::Rating, &BrowseFilter::default()).unwrap(),
            vec![
                BrowseValue {
                    value: "0".into(),
                    count: 1
                },
                BrowseValue {
                    value: "3".into(),
                    count: 1
                },
                BrowseValue {
                    value: "4".into(),
                    count: 1
                },
                BrowseValue {
                    value: "5".into(),
                    count: 2
                },
            ]
        );
    }

    #[test]
    fn year_value_list_reflects_other_active_facets_but_not_year_itself() {
        let conn = seeded_facets();
        // Genre=Rock keeps rows 1,2,3; the standalone year selection is
        // ignored when listing years, but genre still narrows the counts.
        let filter = BrowseFilter {
            genre: Some("Rock".into()),
            year: Some("2001".into()),
            ..BrowseFilter::default()
        };
        assert_eq!(
            query_browse_values(&conn, BrowseFacet::Year, &filter).unwrap(),
            vec![
                BrowseValue {
                    value: "1999".into(),
                    count: 1
                },
                BrowseValue {
                    value: "2001".into(),
                    count: 2
                },
            ]
        );
    }

    #[test]
    fn year_and_rating_bind_as_parameters_and_narrow_the_clause() {
        let filter = BrowseFilter {
            year: Some("2001".into()),
            rating: Some("5".into()),
            ..BrowseFilter::default()
        };
        assert_eq!(
            browse_clause(&filter, 1),
            (
                " AND year = ?1 AND rating = ?2".into(),
                vec!["2001".into(), "5".into()],
            )
        );
    }
}
