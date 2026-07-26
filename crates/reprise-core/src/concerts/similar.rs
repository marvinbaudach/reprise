use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension};
use serde_json::Value;
use url::Url;

use super::candidates::{ArtistCandidate, SeedArtist};
use super::config::SimilarConfig;
use super::{normalize_component, ProviderError};

pub const LB_SIMILAR_ALGORITHM: &str =
    "session_based_days_7500_session_300_contribution_5_threshold_10_limit_100_filter_True_skip_30";
pub const LASTFM_MIN_MATCH: f64 = 0.4;
pub(crate) const SIMILAR_SEEDS: usize = 5;
pub(crate) const MAX_SIMILAR_ARTISTS: usize = 50;

#[derive(Clone, Debug, PartialEq)]
pub struct SimilarArtist {
    pub name: String,
    pub mbid: Option<String>,
    pub score: f64,
}

pub fn listenbrainz_similar_url(mbid: &str) -> String {
    let mut url =
        Url::parse("https://labs.api.listenbrainz.org/similar-artists/json").expect("valid URL");
    url.query_pairs_mut()
        .append_pair("artist_mbids", mbid)
        .append_pair("algorithm", LB_SIMILAR_ALGORITHM);
    url.into()
}

pub fn lastfm_similar_url(name: &str, api_key: &str, limit: usize) -> String {
    let mut url = Url::parse("https://ws.audioscrobbler.com/2.0/").expect("valid URL");
    url.query_pairs_mut()
        .append_pair("method", "artist.getsimilar")
        .append_pair("artist", name)
        .append_pair("api_key", api_key)
        .append_pair("format", "json")
        .append_pair("limit", &limit.to_string());
    url.into()
}

pub fn parse_listenbrainz_similar(body: &str) -> Result<Vec<SimilarArtist>, ProviderError> {
    let value: Value = serde_json::from_str(body).map_err(|_| ProviderError::Parse)?;
    let mut artists = Vec::new();
    collect_listenbrainz_rows(&value, &mut artists);
    sort_by_score(&mut artists);
    Ok(artists)
}

fn collect_listenbrainz_rows(value: &Value, artists: &mut Vec<SimilarArtist>) {
    match value {
        Value::Array(values) => {
            for value in values {
                collect_listenbrainz_rows(value, artists);
            }
        }
        Value::Object(object) => {
            let name = object.get("name").and_then(Value::as_str).map(str::trim);
            let score = object.get("score").and_then(number);
            if let (Some(name), Some(score)) = (name, score) {
                if !name.is_empty() && score.is_finite() {
                    artists.push(SimilarArtist {
                        name: name.to_owned(),
                        mbid: optional_string(object.get("artist_mbid")),
                        score,
                    });
                }
                return;
            }
            for key in ["results", "artists", "similar_artists"] {
                if let Some(value) = object.get(key) {
                    collect_listenbrainz_rows(value, artists);
                }
            }
        }
        _ => {}
    }
}

pub fn parse_lastfm_similar(body: &str) -> Result<Vec<SimilarArtist>, ProviderError> {
    let value: Value = serde_json::from_str(body).map_err(|_| ProviderError::Parse)?;
    let rows = value
        .get("similarartists")
        .and_then(|value| value.get("artist"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut artists = rows
        .iter()
        .filter_map(|row| {
            let name = row.get("name")?.as_str()?.trim();
            let score = number(row.get("match")?)?;
            (!name.is_empty() && score.is_finite() && score >= LASTFM_MIN_MATCH).then(|| {
                SimilarArtist {
                    name: name.to_owned(),
                    mbid: optional_string(row.get("mbid")),
                    score,
                }
            })
        })
        .collect::<Vec<_>>();
    sort_by_score(&mut artists);
    Ok(artists)
}

fn optional_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.trim().parse().ok())
}

fn sort_by_score(artists: &mut [SimilarArtist]) {
    artists.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
}

pub(crate) trait SimilarFetch {
    fn listenbrainz(&self, mbid: &str) -> Result<Vec<SimilarArtist>, ProviderError>;
    fn lastfm(
        &self,
        name: &str,
        api_key: &str,
        limit: usize,
    ) -> Result<Vec<SimilarArtist>, ProviderError>;
}

pub(crate) struct HttpSimilarFetch;

impl SimilarFetch for HttpSimilarFetch {
    fn listenbrainz(&self, mbid: &str) -> Result<Vec<SimilarArtist>, ProviderError> {
        let body = super::http::get(&listenbrainz_similar_url(mbid))?;
        parse_listenbrainz_similar(&body)
    }

    fn lastfm(
        &self,
        name: &str,
        api_key: &str,
        limit: usize,
    ) -> Result<Vec<SimilarArtist>, ProviderError> {
        let body = super::http::get(&lastfm_similar_url(name, api_key, limit))?;
        parse_lastfm_similar(&body)
    }
}

pub(crate) fn similar_candidates(
    conn: &Connection,
    seeds: &[SeedArtist],
    library_artists: &[ArtistCandidate],
    fetch: &dyn SimilarFetch,
    config: SimilarConfig,
    lastfm_api_key: Option<&str>,
) -> Result<Vec<ArtistCandidate>, rusqlite::Error> {
    if !config.enabled {
        return Ok(Vec::new());
    }
    let library_keys = library_artists
        .iter()
        .map(|seed| seed.key.clone())
        .collect::<HashSet<_>>();
    let mut candidates: HashMap<String, (ArtistCandidate, f64)> = HashMap::new();
    for seed in seeds {
        let fetched = match seed.mbid.as_deref() {
            Some(mbid) => fetch.listenbrainz(mbid),
            None => match lastfm_api_key {
                Some(api_key) => fetch.lastfm(&seed.name, api_key, config.count),
                None => continue,
            },
        };
        let mut fetched = match fetched {
            Ok(fetched) => fetched,
            Err(error) => {
                tracing::warn!(seed = seed.name, %error, "similar artist discovery failed");
                continue;
            }
        };
        sort_by_score(&mut fetched);
        let mut accepted_for_seed = 0;
        for artist in fetched {
            let key = normalize_component(&artist.name);
            if key.is_empty() || library_keys.contains(&key) {
                continue;
            }
            if accepted_for_seed >= config.count {
                break;
            }
            accepted_for_seed += 1;
            let candidate = ArtistCandidate {
                key: key.clone(),
                name: artist.name,
                mbid: artist.mbid,
                plays: 0,
                last_attempt_at: conn
                    .query_row(
                        "SELECT last_attempt_at FROM concert_artists WHERE artist_key = ?1",
                        [&key],
                        |row| row.get(0),
                    )
                    .optional()?
                    .flatten(),
                is_similar: true,
                similar_to: Some(seed.name.clone()),
            };
            match candidates.get(&key) {
                Some((_, score)) if *score >= artist.score => {}
                _ => {
                    candidates.insert(key, (candidate, artist.score));
                }
            }
        }
    }
    let mut candidates = candidates.into_values().collect::<Vec<_>>();
    candidates.sort_by(|(left, left_score), (right, right_score)| {
        right_score
            .partial_cmp(left_score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    candidates.truncate(MAX_SIMILAR_ARTISTS);
    Ok(candidates
        .into_iter()
        .map(|(candidate, _)| candidate)
        .collect())
}
