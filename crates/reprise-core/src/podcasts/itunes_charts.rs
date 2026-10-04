//! Apple Podcasts country chart provider.

use std::collections::HashMap;

use serde::Deserialize;

use super::itunes::{self, SearchResult};
use super::PodcastError;

pub const CHART_LIMIT: usize = 12;

const CHART_ENDPOINT: &str = "https://rss.marketingtools.apple.com/api/v2";
const LOOKUP_ENDPOINT: &str = "https://itunes.apple.com/lookup";
const ATTEMPTS_PER_REQUEST: usize = 2;

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

/// The charted ids, in chart order, keeping only those the lookup can actually
/// be asked for — an `i64`. `in_chart_order` needs the same parse to match a
/// row back to its rank, but doing it *here* is what keeps an unusable id out
/// of `lookup_url`'s comma-joined query rather than rejecting it after the
/// request has already been sent.
pub fn parse_chart_ids(json: &str) -> Result<Vec<String>, PodcastError> {
    let response: ChartResponse =
        serde_json::from_str(json).map_err(|error| PodcastError::Parse(error.to_string()))?;
    Ok(response
        .feed
        .results
        .into_iter()
        .map(|row| row.id)
        .filter(|id| id.parse::<i64>().is_ok())
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
    top_podcasts_with(country, &mut super::http::get_json)
}

fn top_podcasts_with(
    country: &str,
    fetch: &mut dyn FnMut(&str) -> Result<super::http::Response, PodcastError>,
) -> Result<Vec<SearchResult>, PodcastError> {
    let storefront = country.to_ascii_lowercase();
    let ids = fetch_step(
        &chart_url(&storefront),
        "chart",
        &storefront,
        fetch,
        parse_chart_ids,
    )?;
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = fetch_step(
        &lookup_url(&ids),
        "lookup",
        &storefront,
        fetch,
        itunes::parse_results_with_ids,
    )?;
    Ok(in_chart_order(&ids, rows))
}

fn fetch_step<T>(
    url: &str,
    step: &'static str,
    storefront: &str,
    fetch: &mut dyn FnMut(&str) -> Result<super::http::Response, PodcastError>,
    parse: impl Fn(&str) -> Result<T, PodcastError>,
) -> Result<T, PodcastError> {
    for attempt in 1..=ATTEMPTS_PER_REQUEST {
        match fetch(url).and_then(|response| parse(&response.body)) {
            Ok(value) => return Ok(value),
            Err(error) => {
                let retrying = attempt < ATTEMPTS_PER_REQUEST && is_transient(&error);
                tracing::warn!(
                    step,
                    storefront,
                    attempt,
                    retrying,
                    status = error_status(&error),
                    reason = error.classify(),
                    "podcast chart request failed"
                );
                if !retrying {
                    return Err(error);
                }
            }
        }
    }
    unreachable!("a chart request always has at least one attempt")
}

fn is_transient(error: &PodcastError) -> bool {
    matches!(
        error,
        PodcastError::Timeout | PodcastError::HttpStatus(500..=599)
    )
}

fn error_status(error: &PodcastError) -> Option<u16> {
    match error {
        PodcastError::HttpStatus(status) | PodcastError::SourceGone(status) => Some(*status),
        PodcastError::RateLimited { .. } => Some(429),
        _ => None,
    }
}

#[cfg(test)]
#[path = "itunes_charts_tests.rs"]
mod tests;
