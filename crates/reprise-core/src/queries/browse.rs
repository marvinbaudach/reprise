//! Exact, bound Genre/Artist/Album facets for the flat Library source.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
}
