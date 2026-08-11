//! Apple Podcasts search provider.

use serde::Deserialize;

use super::PodcastError;

const SEARCH_ENDPOINT: &str = "https://itunes.apple.com/search";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchResult {
    pub title: String,
    pub author: Option<String>,
    pub feed_url: String,
    pub episode_count: Option<u32>,
    pub image_url: Option<String>,
    pub last_episode: Option<i64>,
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    results: Vec<SearchRow>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchRow {
    collection_id: Option<i64>,
    collection_name: Option<String>,
    artist_name: Option<String>,
    feed_url: Option<String>,
    track_count: Option<u32>,
    artwork_url600: Option<String>,
    artwork_url100: Option<String>,
    release_date: Option<String>,
}

/// Whether a value is a storefront code Apple can be asked for: exactly two
/// ASCII letters. Every path that reaches a request — the locale territory
/// below, and the stored location's country code the add dialog prefers over
/// it — passes through this one predicate, so a country cannot be strict in
/// one place and anything-goes in another. `itunes_charts::chart_url`
/// interpolates the value into a URL *path* segment without encoding, and the
/// stored code originates from Nominatim.
#[must_use]
pub fn is_country_code(value: &str) -> bool {
    value.len() == 2
        && value
            .bytes()
            .all(|character| character.is_ascii_alphabetic())
}

#[must_use]
pub fn locale_country(locale: &str) -> String {
    let locale = locale
        .split_once('.')
        .map_or(locale, |(prefix, _)| prefix)
        .split_once('@')
        .map_or_else(
            || locale.split_once('.').map_or(locale, |(prefix, _)| prefix),
            |(prefix, _)| prefix,
        );
    let territory = locale
        .split_once('_')
        .or_else(|| locale.split_once('-'))
        .map(|(_, territory)| territory);
    match territory {
        Some(value) if is_country_code(value) => value.to_ascii_uppercase(),
        _ => "US".to_owned(),
    }
}

#[must_use]
pub fn search_url(terms: &str, country: &str) -> String {
    format!(
        "{SEARCH_ENDPOINT}?media=podcast&term={}&limit=12&country={}",
        crate::musicbrainz::urlencode(terms.trim()),
        crate::musicbrainz::urlencode(country)
    )
}

pub fn search(terms: &str, locale: &str) -> Result<Vec<SearchResult>, PodcastError> {
    search_in_country(terms, &locale_country(locale))
}

pub fn search_in_country(terms: &str, country: &str) -> Result<Vec<SearchResult>, PodcastError> {
    parse_results(&super::http::get_json(&search_url(terms, country))?.body)
}

pub fn parse_results(json: &str) -> Result<Vec<SearchResult>, PodcastError> {
    Ok(parse_results_with_ids(json)?
        .into_iter()
        .map(|(_, result)| result)
        .collect())
}

pub fn parse_results_with_ids(
    json: &str,
) -> Result<Vec<(Option<i64>, SearchResult)>, PodcastError> {
    let response: SearchResponse =
        serde_json::from_str(json).map_err(|error| PodcastError::Parse(error.to_string()))?;
    Ok(response
        .results
        .into_iter()
        .filter_map(|row| {
            let feed_url = row.feed_url.filter(|value| !value.trim().is_empty())?;
            let title = row
                .collection_name
                .filter(|value| !value.trim().is_empty())?;
            Some((
                row.collection_id,
                SearchResult {
                    title,
                    author: row.artist_name.filter(|value| !value.trim().is_empty()),
                    feed_url,
                    episode_count: row.track_count,
                    image_url: row
                        .artwork_url600
                        .or(row.artwork_url100)
                        .filter(|value| !value.trim().is_empty()),
                    last_episode: parse_release_date(row.release_date.as_deref()),
                },
            ))
        })
        .collect())
}

fn parse_release_date(value: Option<&str>) -> Option<i64> {
    value
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|date| date.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_storefront_code_is_exactly_two_ascii_letters() {
        assert!(is_country_code("DE"));
        assert!(is_country_code("de"));
        for rejected in ["", "D", "DEU", "D3", "d-", "Deutschland", "de/", "d é"] {
            assert!(
                !is_country_code(rejected),
                "{rejected:?} is not a storefront"
            );
        }
    }

    #[test]
    fn locale_country_uses_territory_or_us_fallback() {
        assert_eq!(locale_country("de_DE.UTF-8"), "DE");
        assert_eq!(locale_country("en-GB"), "GB");
        assert_eq!(locale_country("C"), "US");
        assert_eq!(locale_country(""), "US");
        assert_eq!(locale_country("broken"), "US");
    }

    #[test]
    fn search_parser_drops_rows_without_feed_url() {
        let rows = parse_results(
            r#"{"results":[
              {"collectionName":"Show","artistName":"Ada","feedUrl":"https://e.test/feed","trackCount":42,
               "artworkUrl600":"https://e.test/show-600.jpg"},
              {"collectionName":"No feed","artistName":"Lin","trackCount":3}
            ]}"#,
        )
        .unwrap();
        assert_eq!(
            rows,
            vec![SearchResult {
                title: "Show".into(),
                author: Some("Ada".into()),
                feed_url: "https://e.test/feed".into(),
                episode_count: Some(42),
                image_url: Some("https://e.test/show-600.jpg".into()),
                last_episode: None,
            }]
        );
    }

    #[test]
    fn src_18_a_search_row_carries_the_date_of_its_newest_episode() {
        let rows = parse_results(
            r#"{"results":[
              {"collectionName":"Fresh","feedUrl":"https://e.test/fresh","releaseDate":"2026-08-04T04:00:00Z"},
              {"collectionName":"Undated","feedUrl":"https://e.test/undated"}
            ]}"#,
        )
        .unwrap();

        assert_eq!(rows[0].last_episode, Some(1_785_816_000));
        assert_eq!(rows[1].last_episode, None);
    }

    #[test]
    fn src_18_a_malformed_release_date_costs_only_its_own_row() {
        let rows = parse_results(
            r#"{"results":[
              {"collectionName":"Before","feedUrl":"https://e.test/before","releaseDate":"2026-08-03T04:00:00Z"},
              {"collectionName":"Malformed","feedUrl":"https://e.test/malformed","releaseDate":"not a date"},
              {"collectionName":"After","feedUrl":"https://e.test/after","releaseDate":"2026-08-05T04:00:00Z"}
            ]}"#,
        )
        .unwrap();

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].last_episode, Some(1_785_729_600));
        assert_eq!(rows[1].last_episode, None);
        assert_eq!(rows[2].last_episode, Some(1_785_902_400));
    }

    #[test]
    fn search_url_and_country_search_agree_on_the_country() {
        let locale_country = locale_country("de_DE.UTF-8");
        assert_eq!(
            search_url("rust audio", &locale_country),
            search_url("rust audio", "DE")
        );

        let _country_search = search_in_country;
        let _locale_search = search;
    }

    #[test]
    fn search_url_contains_country_and_encoded_terms() {
        let url = search_url("rust & audio", "DE");
        assert!(url.contains("term=rust%20%26%20audio"));
        assert!(url.contains("country=DE"));
        assert!(url.contains("limit=12"));
    }
}
