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
    release_groups_page_url(mbid, 0)
}

pub fn release_groups_page_url(mbid: &str, offset: usize) -> String {
    let offset = if offset == 0 {
        String::new()
    } else {
        format!("&offset={offset}")
    };
    format!(
        "https://musicbrainz.org/ws/2/release-group?artist={}&type=album%7Cep%7Csingle&release-group-status=website-default&limit=100&inc=url-rels&fmt=json{offset}",
        musicbrainz::urlencode(mbid),
    )
}

pub fn release_group_detail_url(release_group_mbid: &str) -> String {
    format!(
        "https://musicbrainz.org/ws/2/release-group/{}?inc=releases%2Bmedia&fmt=json",
        musicbrainz::urlencode(release_group_mbid)
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

pub fn parse_release_groups(json: &str, today: NaiveDate) -> Vec<AlbumNews> {
    parse_release_group_page(json, today).map_or_else(Vec::new, |page| page.items)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseGroupPage {
    pub items: Vec<AlbumNews>,
    pub next_offset: Option<usize>,
}

pub(crate) struct PrimaryArtistReleaseGroupPage {
    pub page: ReleaseGroupPage,
    pub excluded_release_group_mbids: Vec<String>,
}

pub fn parse_release_group_page(json: &str, today: NaiveDate) -> Option<ReleaseGroupPage> {
    parse_release_group_page_for_artist(json, today, None).map(|parsed| parsed.page)
}

pub(crate) fn parse_release_group_page_for_primary_artist(
    json: &str,
    today: NaiveDate,
    artist_mbid: &str,
) -> Option<PrimaryArtistReleaseGroupPage> {
    parse_release_group_page_for_artist(json, today, Some(artist_mbid))
}

fn parse_release_group_page_for_artist(
    json: &str,
    today: NaiveDate,
    artist_mbid: Option<&str>,
) -> Option<PrimaryArtistReleaseGroupPage> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return None;
    };
    let groups = value
        .get("release-groups")
        .and_then(serde_json::Value::as_array)?;
    let mut items = Vec::new();
    let mut excluded_release_group_mbids = Vec::new();
    for group in groups {
        let credit_matches = match artist_mbid {
            Some(artist_mbid) => primary_artist_credit_matches(group, artist_mbid),
            None => true,
        };
        if !credit_matches {
            if let Some(mbid) = group.get("id").and_then(serde_json::Value::as_str) {
                excluded_release_group_mbids.push(mbid.to_string());
            }
            continue;
        }
        if let Some(item) = parse_release_group(group, today) {
            items.push(item);
        }
    }
    items.sort_by(|(left, left_date), (right, right_date)| {
        compare_news(left, *left_date, right, *right_date)
    });
    let items = items.into_iter().map(|(item, _)| item).collect();
    let offset = value
        .get("release-group-offset")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or_default();
    let next_offset = value
        .get("release-group-count")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .and_then(|total| {
            let next = offset.saturating_add(groups.len());
            (next < total && !groups.is_empty()).then_some(next)
        });
    Some(PrimaryArtistReleaseGroupPage {
        page: ReleaseGroupPage { items, next_offset },
        excluded_release_group_mbids,
    })
}

fn primary_artist_credit_matches(group: &serde_json::Value, artist_mbid: &str) -> bool {
    let Some(credits) = group
        .get("artist-credit")
        .and_then(serde_json::Value::as_array)
    else {
        // A release-group browse is already scoped to the artist. Preserve
        // compatibility with incomplete responses while using explicit
        // credits whenever MusicBrainz supplies them.
        return true;
    };
    let mut guest_section = false;
    for credit in credits {
        let matches_artist = credit
            .get("artist")
            .and_then(|artist| artist.get("id"))
            .and_then(serde_json::Value::as_str)
            .is_some_and(|id| id.eq_ignore_ascii_case(artist_mbid));
        if matches_artist {
            return !guest_section;
        }
        if credit
            .get("joinphrase")
            .and_then(serde_json::Value::as_str)
            .is_some_and(joinphrase_starts_guest_section)
        {
            guest_section = true;
        }
    }
    false
}

fn joinphrase_starts_guest_section(joinphrase: &str) -> bool {
    let marker = joinphrase
        .trim()
        .trim_start_matches(|character: char| !character.is_ascii_alphabetic())
        .to_ascii_lowercase();
    marker.starts_with("feat")
        || marker == "ft"
        || marker.starts_with("ft.")
        || marker.starts_with("ft ")
}

pub fn parse_release_track_count(json: &str) -> Option<i64> {
    let value = serde_json::from_str::<serde_json::Value>(json).ok()?;
    value
        .get("releases")?
        .as_array()?
        .iter()
        .filter(|release| {
            release
                .get("status")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|status| status.eq_ignore_ascii_case("official"))
        })
        .filter_map(|release| {
            let media = release.get("media")?.as_array()?;
            media
                .iter()
                .try_fold(0_i64, |total, medium| {
                    let count = medium
                        .get("track-count")
                        .and_then(serde_json::Value::as_i64)
                        .filter(|count| *count > 0)?;
                    total.checked_add(count)
                })
                .filter(|total| *total >= 2)
        })
        .min()
}

pub(crate) fn sort_release_groups(items: &mut [AlbumNews]) {
    items.sort_by(|left, right| {
        let left_date = parse_partial_date(&left.first_release_date);
        let right_date = parse_partial_date(&right.first_release_date);
        match (left_date, right_date) {
            (Some(left_date), Some(right_date)) => compare_news(left, left_date, right, right_date),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => left.title.cmp(&right.title),
        }
    });
}

fn parse_release_group(
    group: &serde_json::Value,
    today: NaiveDate,
) -> Option<(AlbumNews, NaiveDate)> {
    let mbid = group.get("id")?.as_str()?.to_string();
    let title = group.get("title")?.as_str()?.trim().to_string();
    let primary_type = group.get("primary-type")?.as_str()?.to_string();
    let primary_type_normalized = primary_type.to_ascii_lowercase();
    if !matches!(primary_type_normalized.as_str(), "album" | "ep" | "single")
        || title.is_empty()
        || has_excluded_secondary_type(group)
    {
        return None;
    }
    let date_text = group
        .get("first-release-date")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();
    let (kind, release_date) = match parse_partial_date(&date_text) {
        Some(release_date) => (
            release_kind(&primary_type_normalized, &date_text, release_date, today)?,
            release_date,
        ),
        None if matches!(primary_type_normalized.as_str(), "album" | "ep") => {
            (NewsKind::Catalog, NaiveDate::MIN)
        }
        None => return None,
    };
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

pub(crate) fn release_kind(
    primary_type: &str,
    date_text: &str,
    release_date: NaiveDate,
    today: NaiveDate,
) -> Option<NewsKind> {
    let delta = release_date.signed_duration_since(today).num_days();
    match primary_type {
        // An announced single needs an exact date to be trustworthy.
        "single" if date_text.len() == 10 && delta > 0 => Some(NewsKind::Upcoming),
        "single" => Some(NewsKind::Catalog),
        _ if delta >= 0 => Some(NewsKind::Upcoming),
        _ if delta >= -NEWS_WINDOW_DAYS => Some(NewsKind::New),
        _ => Some(NewsKind::Catalog),
    }
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
        (NewsKind::Upcoming, NewsKind::Catalog) => Ordering::Less,
        (NewsKind::New, NewsKind::Upcoming) => Ordering::Greater,
        (NewsKind::New, NewsKind::Catalog) => Ordering::Less,
        (NewsKind::Catalog, NewsKind::Upcoming | NewsKind::New) => Ordering::Greater,
        (NewsKind::Upcoming, NewsKind::Upcoming) => left_date.cmp(&right_date),
        (NewsKind::New, NewsKind::New) => right_date.cmp(&left_date),
        (NewsKind::Catalog, NewsKind::Catalog) => right_date.cmp(&left_date),
    }
    .then_with(|| left.title.cmp(&right.title))
}

pub(crate) fn artist_payload_valid(body: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value.get("artists").cloned())
        .is_some_and(|artists| artists.is_array())
}
