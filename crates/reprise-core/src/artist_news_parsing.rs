//! MusicBrainz JSON parsing, the URL builders for artist search and
//! release-group browse, and the release-comparison/sorting helpers that
//! order parsed results. Split out of `artist_news.rs` purely to stay under
//! the project's 800-line rule; re-exported from there so existing callers
//! keep using `artist_news::{parse_release_groups, ArtistMatch, ...}`.

use std::cmp::Ordering;

use chrono::NaiveDate;

use crate::artist_news::{normalize, AlbumNews, NewsKind};
use crate::musicbrainz;

const MIN_ARTIST_SCORE: i64 = 95;
const NEWS_WINDOW_DAYS: i64 = 90;
const MAX_ITEMS: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtistMatch {
    Found(String),
    Ambiguous,
    NotFound,
}

pub fn artist_search_url(artist: &str) -> String {
    let escaped = artist.trim().replace('\\', "\\\\").replace('"', "\\\"");
    let query = format!("artist:\"{escaped}\"");
    format!(
        "https://musicbrainz.org/ws/2/artist/?query={}&fmt=json&limit=5",
        musicbrainz::urlencode(&query)
    )
}

pub fn release_groups_url(mbid: &str) -> String {
    format!(
        "https://musicbrainz.org/ws/2/release-group?artist={}&type=album%7Cep%7Csingle&release-group-status=website-default&limit=100&inc=url-rels&fmt=json",
        musicbrainz::urlencode(mbid)
    )
}

pub fn parse_artist_mbid(json: &str, artist: &str) -> ArtistMatch {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return ArtistMatch::NotFound;
    };
    let Some(artists) = value.get("artists").and_then(serde_json::Value::as_array) else {
        return ArtistMatch::NotFound;
    };
    let wanted = normalize(artist);
    let mut ids = artists
        .iter()
        .filter(|candidate| {
            candidate
                .get("score")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or_default()
                >= MIN_ARTIST_SCORE
        })
        .filter(|candidate| {
            candidate
                .get("name")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|name| normalize(name) == wanted)
        })
        .filter_map(|candidate| candidate.get("id").and_then(serde_json::Value::as_str))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    match ids.as_slice() {
        [id] => ArtistMatch::Found(id.clone()),
        [] => ArtistMatch::NotFound,
        _ => ArtistMatch::Ambiguous,
    }
}

pub fn parse_release_groups(
    json: &str,
    local_albums: &[String],
    today: NaiveDate,
    include_singles: bool,
) -> Vec<AlbumNews> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let Some(groups) = value
        .get("release-groups")
        .and_then(serde_json::Value::as_array)
    else {
        return Vec::new();
    };
    let local = local_albums
        .iter()
        .map(|album| normalize(album))
        .collect::<std::collections::HashSet<_>>();
    let mut items = groups
        .iter()
        .filter_map(|group| parse_release_group(group, &local, today, include_singles))
        .collect::<Vec<_>>();
    items.sort_by(|(left, left_date), (right, right_date)| {
        compare_news(left, *left_date, right, *right_date)
    });
    items.truncate(MAX_ITEMS);
    items.into_iter().map(|(item, _)| item).collect()
}

fn parse_release_group(
    group: &serde_json::Value,
    local: &std::collections::HashSet<String>,
    today: NaiveDate,
    include_singles: bool,
) -> Option<(AlbumNews, NaiveDate)> {
    let mbid = group.get("id")?.as_str()?.to_string();
    let title = group.get("title")?.as_str()?.trim().to_string();
    let date_text = group.get("first-release-date")?.as_str()?.to_string();
    let release_date = parse_partial_date(&date_text)?;
    let primary_type = group.get("primary-type")?.as_str()?.to_string();
    let primary_type_normalized = primary_type.to_ascii_lowercase();
    if !matches!(primary_type_normalized.as_str(), "album" | "ep" | "single")
        || title.is_empty()
        || has_excluded_secondary_type(group)
    {
        return None;
    }
    let delta = release_date.signed_duration_since(today).num_days();
    let kind = match primary_type_normalized.as_str() {
        // An announced single needs an exact date to be trustworthy; that
        // rule predates the switch and stays on unconditionally, so turning
        // the switch off never shows *less* than before.
        "single" if date_text.len() == 10 && delta > 0 => NewsKind::Upcoming,
        "single" if !include_singles => return None,
        "single" if delta >= -NEWS_WINDOW_DAYS => NewsKind::New,
        "single" => return None,
        _ if delta >= 0 => NewsKind::Upcoming,
        _ if delta >= -NEWS_WINDOW_DAYS => NewsKind::New,
        _ => return None,
    };
    // An unreleased album cannot be owned. A title match here is by
    // definition a mis-tagged pre-release track — typically the lead single
    // tagged with the forthcoming album's name — so the library check is
    // skipped outright rather than merely relaxed.
    if kind == NewsKind::New && local.contains(&normalize(&title)) {
        return None;
    }
    Some((
        AlbumNews {
            release_group_mbid: mbid,
            title,
            first_release_date: date_text,
            primary_type,
            kind,
            announce_url: crate::artist_news_links::parse_announce_url(group),
        },
        release_date,
    ))
}

fn has_excluded_secondary_type(group: &serde_json::Value) -> bool {
    const EXCLUDED: &[&str] = &[
        "compilation",
        "live",
        "remix",
        "soundtrack",
        "mixtape/street",
        "dj-mix",
    ];
    group
        .get("secondary-types")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|types| {
            types.iter().any(|value| {
                value
                    .as_str()
                    .is_some_and(|kind| EXCLUDED.contains(&kind.to_ascii_lowercase().as_str()))
            })
        })
}

pub(crate) fn parse_partial_date(value: &str) -> Option<NaiveDate> {
    match value.len() {
        10 => NaiveDate::parse_from_str(value, "%Y-%m-%d").ok(),
        7 => NaiveDate::parse_from_str(&format!("{value}-01"), "%Y-%m-%d").ok(),
        4 => NaiveDate::parse_from_str(&format!("{value}-01-01"), "%Y-%m-%d").ok(),
        _ => None,
    }
}

fn compare_news(
    left: &AlbumNews,
    left_date: NaiveDate,
    right: &AlbumNews,
    right_date: NaiveDate,
) -> Ordering {
    match (left.kind, right.kind) {
        (NewsKind::Upcoming, NewsKind::New) => Ordering::Less,
        (NewsKind::New, NewsKind::Upcoming) => Ordering::Greater,
        (NewsKind::Upcoming, NewsKind::Upcoming) => left_date.cmp(&right_date),
        (NewsKind::New, NewsKind::New) => right_date.cmp(&left_date),
    }
    .then_with(|| left.title.cmp(&right.title))
}

pub(crate) fn artist_payload_valid(body: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value.get("artists").cloned())
        .is_some_and(|artists| artists.is_array())
}

pub(crate) fn release_payload_valid(body: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value.get("release-groups").cloned())
        .is_some_and(|groups| groups.is_array())
}
