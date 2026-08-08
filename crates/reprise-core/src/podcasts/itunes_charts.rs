//! Apple Podcasts country chart provider.

use std::collections::HashMap;

use serde::Deserialize;

use super::itunes::{self, SearchResult};
use super::PodcastError;

pub const CHART_LIMIT: usize = 12;

const CHART_ENDPOINT: &str = "https://rss.marketingtools.apple.com/api/v2";
const LOOKUP_ENDPOINT: &str = "https://itunes.apple.com/lookup";

#[derive(Deserialize)]
struct ChartResponse {
    feed: ChartFeed,
}

#[derive(Deserialize)]
struct ChartFeed {
    #[serde(default)]
    results: Vec<ChartRow>,
}

#[derive(Deserialize)]
struct ChartRow {
    id: String,
}

#[must_use]
pub fn chart_url(country: &str) -> String {
    format!(
        "{CHART_ENDPOINT}/{}/podcasts/top/{CHART_LIMIT}/podcasts.json",
        country.to_ascii_lowercase()
    )
}

#[must_use]
pub fn lookup_url(ids: &[String]) -> String {
    format!("{LOOKUP_ENDPOINT}?id={}&entity=podcast", ids.join(","))
}

pub fn parse_chart_ids(json: &str) -> Result<Vec<String>, PodcastError> {
    let response: ChartResponse =
        serde_json::from_str(json).map_err(|error| PodcastError::Parse(error.to_string()))?;
    Ok(response
        .feed
        .results
        .into_iter()
        .map(|row| row.id)
        .collect())
}

#[must_use]
pub fn in_chart_order(ids: &[String], rows: Vec<(Option<i64>, SearchResult)>) -> Vec<SearchResult> {
    let mut rows_by_id = rows
        .into_iter()
        .filter_map(|(id, row)| id.map(|id| (id, row)))
        .collect::<HashMap<_, _>>();
    ids.iter()
        .filter_map(|id| id.parse::<i64>().ok())
        .filter_map(|id| rows_by_id.remove(&id))
        .collect()
}

pub fn top_podcasts(country: &str) -> Result<Vec<SearchResult>, PodcastError> {
    let chart = super::http::get(&chart_url(country))?;
    let ids = parse_chart_ids(&chart.body)?;
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let lookup = super::http::get(&lookup_url(&ids))?;
    let rows = itunes::parse_results_with_ids(&lookup.body)?;
    Ok(in_chart_order(&ids, rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::podcasts::itunes::SearchResult;

    fn result(title: &str, feed_url: &str) -> SearchResult {
        SearchResult {
            title: title.to_owned(),
            author: None,
            feed_url: feed_url.to_owned(),
            episode_count: None,
            image_url: None,
            last_episode: None,
        }
    }

    #[test]
    fn src_19_the_chart_request_uses_the_lowercase_storefront_code() {
        assert_eq!(
            chart_url("DE"),
            "https://rss.marketingtools.apple.com/api/v2/de/podcasts/top/12/podcasts.json"
        );
    }

    #[test]
    fn src_19_the_lookup_batches_every_charted_id_into_one_request() {
        let ids = (1..=12).map(|id| id.to_string()).collect::<Vec<_>>();
        let url = lookup_url(&ids);

        assert!(url.contains("id=1,2,3,4,5,6,7,8,9,10,11,12"));
        assert!(url.contains("entity=podcast"));
        assert_eq!(url.matches("id=").count(), 1);
    }

    #[test]
    fn src_19_chart_ids_are_read_in_chart_order() {
        let ids = parse_chart_ids(r#"{"feed":{"results":[{"id":"42"},{"id":"7"},{"id":"99"}]}}"#)
            .unwrap();

        assert_eq!(ids, ["42", "7", "99"]);
    }

    #[test]
    fn src_19_the_lookup_answer_is_restored_to_chart_order() {
        let rows = vec![
            (Some(7), result("Seven", "https://e.test/7")),
            (Some(42), result("Forty-two", "https://e.test/42")),
            (Some(99), result("Ninety-nine", "https://e.test/99")),
        ];

        let ordered = in_chart_order(&["42".into(), "7".into(), "99".into()], rows);

        assert_eq!(
            ordered
                .iter()
                .map(|row| row.title.as_str())
                .collect::<Vec<_>>(),
            ["Forty-two", "Seven", "Ninety-nine"]
        );
    }

    #[test]
    fn src_19_an_id_the_lookup_drops_falls_out_rather_than_leaving_a_hole() {
        let ids = (1..=12).map(|id| id.to_string()).collect::<Vec<_>>();
        let rows = (1..=12)
            .filter(|id| *id != 6)
            .rev()
            .map(|id| {
                (
                    Some(id),
                    result(&format!("Show {id}"), &format!("https://e.test/{id}")),
                )
            })
            .collect();

        let ordered = in_chart_order(&ids, rows);

        assert_eq!(ordered.len(), 11);
        assert_eq!(
            ordered
                .iter()
                .map(|row| row.title.as_str())
                .collect::<Vec<_>>(),
            [
                "Show 1", "Show 2", "Show 3", "Show 4", "Show 5", "Show 7", "Show 8", "Show 9",
                "Show 10", "Show 11", "Show 12"
            ]
        );
    }

    #[test]
    fn malformed_chart_body_is_a_parse_error() {
        let error = parse_chart_ids(r#"{"feed":{"results":not-json}}"#).unwrap_err();
        assert!(matches!(error, crate::podcasts::PodcastError::Parse(_)));
    }
}
