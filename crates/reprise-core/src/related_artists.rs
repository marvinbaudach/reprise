//! Opt-in ListenBrainz related-artist discovery, cache, and library filtering.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::library::{group_key, settings};

pub const PROVIDER_NAME: &str = "ListenBrainz";
pub const MAX_SUGGESTIONS: usize = 20;
const MAX_SEEDS: usize = 10;
const CACHE_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;
const MAX_RESPONSE_BYTES: u64 = 512 * 1024;
const HIDDEN_KEY: &str = "related_artists.hidden";

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CacheEntry {
    fetched_at: i64,
    suggestions: Vec<RelatedArtistSuggestion>,
}

#[derive(Deserialize)]
struct ProviderRow {
    similar_artist_mbid: String,
    similar_artist_name: String,
    total_listen_count: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RelatedArtistSuggestion {
    pub artist_mbid: String,
    pub artist_name: String,
    pub seed_artist_mbid: String,
    pub seed_artist_name: String,
    pub total_listen_count: i64,
    pub source: String,
    pub reason: String,
}

#[derive(Debug, thiserror::Error)]
pub enum RelatedArtistError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("invalid provider response: {0}")]
    Json(#[from] serde_json::Error),
    #[error("provider request failed: {0}")]
    Network(String),
}

pub fn discover_related_artists<F>(
    conn: &Connection,
    seed_track_ids: &[i64],
    now: i64,
    mut fetch: F,
) -> Result<Vec<RelatedArtistSuggestion>, RelatedArtistError>
where
    F: FnMut(&str) -> Result<String, RelatedArtistError>,
{
    if !crate::modules::is_enabled(conn, &crate::modules::RELATED_ARTISTS_MODULE)? {
        return Ok(Vec::new());
    }
    let seeds = seed_artists(conn, seed_track_ids)?;
    let (library_mbids, library_names) = library_identities(conn)?;
    let mut suggestions = HashMap::<String, RelatedArtistSuggestion>::new();
    for (seed_mbid, seed_name) in seeds.into_iter().take(MAX_SEEDS) {
        let cache_key = format!("related_artists.cache.{seed_mbid}");
        let cached = settings::get_setting(conn, &cache_key)?
            .and_then(|json| serde_json::from_str::<CacheEntry>(&json).ok())
            .filter(|entry| now.saturating_sub(entry.fetched_at) <= CACHE_TTL_SECONDS);
        let entry = match cached {
            Some(entry) => entry,
            None => {
                let url = provider_url(&seed_mbid);
                let body = fetch(&url)?;
                let entry = CacheEntry {
                    fetched_at: now,
                    suggestions: parse_provider(&body, &seed_mbid, &seed_name)?,
                };
                settings::set_setting(conn, &cache_key, &serde_json::to_string(&entry)?)?;
                entry
            }
        };
        for suggestion in entry.suggestions {
            let normalized_name = group_key::normalize_group_key(&suggestion.artist_name);
            if suggestion.artist_mbid == seed_mbid
                || library_mbids.contains(&suggestion.artist_mbid)
                || library_names.contains(&normalized_name)
            {
                continue;
            }
            suggestions
                .entry(suggestion.artist_mbid.clone())
                .and_modify(|current| {
                    if suggestion.total_listen_count > current.total_listen_count {
                        *current = suggestion.clone();
                    }
                })
                .or_insert(suggestion);
        }
    }
    let mut suggestions = suggestions.into_values().collect::<Vec<_>>();
    suggestions.sort_by(|left, right| {
        right
            .total_listen_count
            .cmp(&left.total_listen_count)
            .then_with(|| left.artist_name.cmp(&right.artist_name))
            .then_with(|| left.artist_mbid.cmp(&right.artist_mbid))
    });
    suggestions.truncate(MAX_SUGGESTIONS);
    Ok(suggestions)
}

pub fn provider_urls(
    conn: &Connection,
    seed_track_ids: &[i64],
) -> Result<Vec<String>, RelatedArtistError> {
    if !crate::modules::is_enabled(conn, &crate::modules::RELATED_ARTISTS_MODULE)? {
        return Ok(Vec::new());
    }
    Ok(seed_artists(conn, seed_track_ids)?
        .into_iter()
        .take(MAX_SEEDS)
        .map(|(mbid, _)| provider_url(&mbid))
        .collect())
}

pub fn provider_urls_needing_fetch(
    conn: &Connection,
    seed_track_ids: &[i64],
    now: i64,
) -> Result<Vec<String>, RelatedArtistError> {
    if !crate::modules::is_enabled(conn, &crate::modules::RELATED_ARTISTS_MODULE)? {
        return Ok(Vec::new());
    }
    let mut urls = Vec::new();
    for (seed_mbid, _) in seed_artists(conn, seed_track_ids)?
        .into_iter()
        .take(MAX_SEEDS)
    {
        let cache_key = format!("related_artists.cache.{seed_mbid}");
        let fresh = settings::get_setting(conn, &cache_key)?
            .and_then(|json| serde_json::from_str::<CacheEntry>(&json).ok())
            .is_some_and(|entry| now.saturating_sub(entry.fetched_at) <= CACHE_TTL_SECONDS);
        if !fresh {
            urls.push(provider_url(&seed_mbid));
        }
    }
    Ok(urls)
}

pub fn fetch_listenbrainz(url: &str) -> Result<String, RelatedArtistError> {
    let response = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .https_only(true)
        .user_agent(crate::musicbrainz::user_agent())
        .build()
        .new_agent()
        .get(url)
        .call()
        .map_err(|error| RelatedArtistError::Network(error.to_string()))?;
    let mut body = String::new();
    response
        .into_body()
        .into_reader()
        .take(MAX_RESPONSE_BYTES + 1)
        .read_to_string(&mut body)
        .map_err(|error| RelatedArtistError::Network(error.to_string()))?;
    if body.len() as u64 > MAX_RESPONSE_BYTES {
        return Err(RelatedArtistError::Network(
            "provider response exceeded the size limit".into(),
        ));
    }
    Ok(body)
}

pub fn set_hidden(
    conn: &Connection,
    artist_mbid: &str,
    hidden: bool,
) -> Result<(), RelatedArtistError> {
    let mut mbids = hidden_artist_mbids(conn)?;
    if hidden {
        if !mbids.iter().any(|value| value == artist_mbid) {
            mbids.push(artist_mbid.to_string());
        }
    } else {
        mbids.retain(|value| value != artist_mbid);
    }
    mbids.sort();
    mbids.dedup();
    settings::set_setting(conn, HIDDEN_KEY, &serde_json::to_string(&mbids)?)?;
    Ok(())
}

pub fn hidden_artist_mbids(conn: &Connection) -> Result<Vec<String>, RelatedArtistError> {
    Ok(settings::get_setting(conn, HIDDEN_KEY)?
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default())
}

pub fn cached_local_track_ids(
    conn: &Connection,
    seed_track_ids: &[i64],
) -> Result<Vec<i64>, RelatedArtistError> {
    let seeds = seed_artists(conn, seed_track_ids)?;
    let mut related_mbids = HashSet::new();
    let mut related_names = HashSet::new();
    for (seed_mbid, _) in seeds {
        let cache_key = format!("related_artists.cache.{seed_mbid}");
        let Some(entry) = settings::get_setting(conn, &cache_key)?
            .and_then(|json| serde_json::from_str::<CacheEntry>(&json).ok())
        else {
            continue;
        };
        for suggestion in entry.suggestions {
            related_mbids.insert(suggestion.artist_mbid);
            related_names.insert(group_key::normalize_group_key(&suggestion.artist_name));
        }
    }
    let mut statement = conn.prepare(
        "SELECT id, artist_mbid, artist FROM tracks
         WHERE removed_at IS NULL AND missing_since IS NULL ORDER BY id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut ids = Vec::new();
    for row in rows {
        let (id, mbid, name) = row?;
        if mbid
            .as_ref()
            .is_some_and(|mbid| related_mbids.contains(mbid))
            || related_names.contains(&group_key::normalize_group_key(&name))
        {
            ids.push(id);
        }
    }
    Ok(ids)
}

fn seed_artists(
    conn: &Connection,
    track_ids: &[i64],
) -> Result<Vec<(String, String)>, RelatedArtistError> {
    let mut seeds = Vec::new();
    for track_id in track_ids.iter().copied().take(MAX_SEEDS) {
        let row = conn
            .query_row(
                "SELECT trim(artist_mbid), trim(artist) FROM tracks
                 WHERE id = ?1 AND artist_mbid IS NOT NULL
                   AND removed_at IS NULL AND missing_since IS NULL",
                [track_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        if let Some((mbid, name)) = row {
            if valid_mbid(&mbid) && !seeds.iter().any(|(known, _)| known == &mbid) {
                seeds.push((mbid, name));
            }
        }
    }
    Ok(seeds)
}

fn library_identities(
    conn: &Connection,
) -> Result<(HashSet<String>, HashSet<String>), RelatedArtistError> {
    let mut statement = conn.prepare(
        "SELECT artist_mbid, artist FROM tracks WHERE removed_at IS NULL GROUP BY artist_mbid, artist",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, Option<String>>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut mbids = HashSet::new();
    let mut names = HashSet::new();
    for row in rows {
        let (mbid, name) = row?;
        if let Some(mbid) = mbid.filter(|value| !value.trim().is_empty()) {
            mbids.insert(mbid);
        }
        names.insert(group_key::normalize_group_key(&name));
    }
    Ok((mbids, names))
}

fn parse_provider(
    body: &str,
    seed_mbid: &str,
    seed_name: &str,
) -> Result<Vec<RelatedArtistSuggestion>, RelatedArtistError> {
    let rows: Vec<ProviderRow> = serde_json::from_str(body)?;
    let mut by_artist = HashMap::<String, RelatedArtistSuggestion>::new();
    for row in rows {
        if !valid_mbid(&row.similar_artist_mbid) || row.similar_artist_name.trim().is_empty() {
            continue;
        }
        let suggestion = RelatedArtistSuggestion {
            artist_mbid: row.similar_artist_mbid,
            artist_name: row.similar_artist_name.trim().to_string(),
            seed_artist_mbid: seed_mbid.to_string(),
            seed_artist_name: seed_name.to_string(),
            total_listen_count: row.total_listen_count.max(0),
            source: PROVIDER_NAME.to_string(),
            reason: format!("Related to {seed_name}"),
        };
        by_artist
            .entry(suggestion.artist_mbid.clone())
            .and_modify(|current| {
                if suggestion.total_listen_count > current.total_listen_count {
                    *current = suggestion.clone();
                }
            })
            .or_insert(suggestion);
    }
    Ok(by_artist.into_values().collect())
}

fn provider_url(seed_mbid: &str) -> String {
    format!(
        "https://api.listenbrainz.org/1/lb-radio/artist/{seed_mbid}?mode=easy&max_similar_artists={MAX_SUGGESTIONS}&max_recordings_per_artist=1&pop_begin=0&pop_end=100"
    )
}

fn valid_mbid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use rusqlite::{params, Connection};

    use super::*;

    const SEED: &str = "11111111-1111-1111-1111-111111111111";
    const LOCAL: &str = "22222222-2222-2222-2222-222222222222";
    const NEW: &str = "33333333-3333-3333-3333-333333333333";

    fn fixture() -> Connection {
        let conn = crate::db::open_migrated(None).unwrap();
        for (id, name, mbid) in [(1, "Seed Artist", SEED), (2, "Local Artist", LOCAL)] {
            conn.execute(
                "INSERT INTO tracks
                 (id, path, title, artist, artist_mbid, album, added_at)
                 VALUES (?1, ?2, 'Track', ?3, ?4, 'Album', 1)",
                params![id, format!("/fixture/{id}.flac"), name, mbid],
            )
            .unwrap();
        }
        conn
    }

    fn response() -> String {
        format!(
            r#"[
              {{"recording_mbid":"a","similar_artist_mbid":"{SEED}","similar_artist_name":"Seed Artist","total_listen_count":999}},
              {{"recording_mbid":"b","similar_artist_mbid":"{LOCAL}","similar_artist_name":"Local Artist","total_listen_count":500}},
              {{"recording_mbid":"c","similar_artist_mbid":"{NEW}","similar_artist_name":"New Artist","total_listen_count":400}},
              {{"recording_mbid":"d","similar_artist_mbid":"44444444-4444-4444-4444-444444444444","similar_artist_name":" local   artist ","total_listen_count":300}}
            ]"#
        )
    }

    #[test]
    fn ac_17_disabled_module_never_calls_the_provider() {
        let conn = fixture();
        let called = Cell::new(false);
        let suggestions = discover_related_artists(&conn, &[1], 100, |_| {
            called.set(true);
            Ok(response())
        })
        .unwrap();
        assert!(suggestions.is_empty());
        assert!(!called.get());
    }

    #[test]
    fn provider_results_exclude_seed_and_library_artist_identities_and_use_cache() {
        let conn = fixture();
        crate::modules::set_enabled(&conn, &crate::modules::RELATED_ARTISTS_MODULE, true).unwrap();
        let calls = Cell::new(0);
        let first = discover_related_artists(&conn, &[1], 100, |_| {
            calls.set(calls.get() + 1);
            Ok(response())
        })
        .unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].artist_mbid, NEW);
        assert_eq!(first[0].seed_artist_name, "Seed Artist");
        assert_eq!(first[0].source, PROVIDER_NAME);

        let second = discover_related_artists(&conn, &[1], 101, |_| {
            panic!("fresh cache must avoid a second provider call")
        })
        .unwrap();
        assert_eq!(second, first);
        assert_eq!(calls.get(), 1);
        assert!(provider_urls_needing_fetch(&conn, &[1], 101)
            .unwrap()
            .is_empty());
        assert_eq!(cached_local_track_ids(&conn, &[1]).unwrap(), vec![1, 2]);

        let intent = crate::mix_planner::MixIntent::new(
            crate::mix_planner::MixSource::Library,
            vec![1],
            crate::mix_planner::CriteriaMode::RelatedArtists,
            crate::mix_planner::ProfileTarget::neutral(),
            180_000,
            crate::mix_planner::Familiarity::Balanced,
            crate::mix_planner::Variety::Balanced,
            crate::mix_planner::EnergyCurve::Flat,
        )
        .unwrap();
        let draft = crate::mix_planner::plan_mix(&conn, &intent).unwrap();
        assert_eq!(draft.tracks.len(), 1);
        assert_eq!(draft.tracks[0].track_id, 2);
        assert!(draft.tracks[0]
            .reasons
            .contains(&crate::mix_planner::SelectionReason::RelatedArtist));
    }

    #[test]
    fn hidden_suggestions_can_be_restored() {
        let conn = fixture();
        set_hidden(&conn, NEW, true).unwrap();
        assert_eq!(hidden_artist_mbids(&conn).unwrap(), vec![NEW]);
        set_hidden(&conn, NEW, false).unwrap();
        assert!(hidden_artist_mbids(&conn).unwrap().is_empty());
    }
}
