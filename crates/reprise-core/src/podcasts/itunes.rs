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
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    results: Vec<SearchRow>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchRow {
    collection_name: Option<String>,
    artist_name: Option<String>,
    feed_url: Option<String>,
    track_count: Option<u32>,
    artwork_url600: Option<String>,
    artwork_url100: Option<String>,
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
        Some(value)
            if value.len() == 2
                && value
                    .bytes()
                    .all(|character| character.is_ascii_alphabetic()) =>
        {
            value.to_ascii_uppercase()
        }
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
    let country = locale_country(locale);
    parse_results(&super::http::get(&search_url(terms, &country))?.body)
}

pub fn parse_results(json: &str) -> Result<Vec<SearchResult>, PodcastError> {
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
            Some(SearchResult {
                title,
                author: row.artist_name.filter(|value| !value.trim().is_empty()),
                feed_url,
                episode_count: row.track_count,
                image_url: row
                    .artwork_url600
                    .or(row.artwork_url100)
                    .filter(|value| !value.trim().is_empty()),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

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
            }]
        );
    }

    #[test]
    fn search_url_contains_country_and_encoded_terms() {
        let url = search_url("rust & audio", "DE");
        assert!(url.contains("term=rust%20%26%20audio"));
        assert!(url.contains("country=DE"));
        assert!(url.contains("limit=12"));
    }
}
