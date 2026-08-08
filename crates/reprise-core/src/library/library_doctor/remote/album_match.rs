use std::cmp::Ordering;
use std::collections::BTreeSet;

use super::guard_rails::is_placeholder_artist;
use super::{ReleaseSecondaryType, RemoteIdentity};
use crate::library::group_key::normalize_group_key;

pub(crate) const TRACK_COUNT_WEIGHT: u8 = 40;
pub(crate) const TITLE_OVERLAP_WEIGHT: u8 = 30;
pub(crate) const ARTIST_CREDIT_WEIGHT: u8 = 20;
pub(crate) const YEAR_PROXIMITY_WEIGHT: u8 = 10;
pub(crate) const COMPILATION_PENALTY: u8 = 30;
const YEAR_ONE_OFF_SCORE: u8 = 8;
const YEAR_TWO_OFF_SCORE: u8 = 6;
const YEAR_NEAR_SCORE: u8 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AlbumQuery {
    pub(crate) album_artist: String,
    pub(crate) album: String,
    pub(crate) track_titles: Vec<String>,
    pub(crate) track_count: u32,
    pub(crate) year: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AlbumMatch {
    pub(crate) identity: RemoteIdentity,
    pub(crate) score: u8,
    pub(crate) exact: bool,
}

pub(crate) fn best_release(
    query: &AlbumQuery,
    candidates: &[RemoteIdentity],
) -> Option<AlbumMatch> {
    candidates
        .iter()
        .map(|identity| score_release(query, identity))
        .min_by(compare_matches)
}

fn score_release(query: &AlbumQuery, identity: &RemoteIdentity) -> AlbumMatch {
    let track_count_matches = identity.release_track_count == Some(query.track_count);
    let title_overlap = title_overlap(query, identity);
    let artist_matches = identity
        .album_artist
        .as_deref()
        .is_some_and(|artist| same_group_key(artist, &query.album_artist));

    let mut score = 0_u8;
    if track_count_matches {
        score = score.saturating_add(TRACK_COUNT_WEIGHT);
    }
    score = score.saturating_add(weighted_fraction(TITLE_OVERLAP_WEIGHT, title_overlap));
    if artist_matches {
        score = score.saturating_add(ARTIST_CREDIT_WEIGHT);
    }
    score = score.saturating_add(year_score(query.year, identity.release_year));

    if is_penalized_release(identity) && !local_tags_describe_special_release(query) {
        score = score.saturating_sub(COMPILATION_PENALTY);
    }

    AlbumMatch {
        identity: identity.clone(),
        score,
        exact: track_count_matches && title_overlap == 1.0 && artist_matches,
    }
}

fn compare_matches(left: &AlbumMatch, right: &AlbumMatch) -> Ordering {
    right.score.cmp(&left.score).then_with(|| {
        stable_identity_key(&left.identity).cmp(&stable_identity_key(&right.identity))
    })
}

fn stable_identity_key(identity: &RemoteIdentity) -> String {
    format!("{identity:?}")
}

fn title_overlap(query: &AlbumQuery, identity: &RemoteIdentity) -> f64 {
    let local = normalized_titles(&query.track_titles);
    if local.is_empty() {
        return 0.0;
    }
    let remote = normalized_titles(&identity.release_track_titles);
    local.intersection(&remote).count() as f64 / local.len() as f64
}

fn normalized_titles(titles: &[String]) -> BTreeSet<String> {
    titles
        .iter()
        .map(|title| normalize_group_key(title))
        .filter(|title| !title.is_empty())
        .collect()
}

fn weighted_fraction(weight: u8, fraction: f64) -> u8 {
    (f64::from(weight) * fraction).round() as u8
}

fn year_score(local: Option<u32>, remote: Option<u32>) -> u8 {
    let (Some(local), Some(remote)) = (local, remote) else {
        return 0;
    };
    match local.abs_diff(remote) {
        0 => YEAR_PROXIMITY_WEIGHT,
        1 => YEAR_ONE_OFF_SCORE,
        2 => YEAR_TWO_OFF_SCORE,
        3..=5 => YEAR_NEAR_SCORE,
        _ => 0,
    }
}

fn is_penalized_release(identity: &RemoteIdentity) -> bool {
    identity.secondary_types.iter().any(|kind| {
        matches!(
            kind,
            ReleaseSecondaryType::Compilation
                | ReleaseSecondaryType::DjMix
                | ReleaseSecondaryType::Live
                | ReleaseSecondaryType::Mixtape
                | ReleaseSecondaryType::Remix
        )
    })
}

fn local_tags_describe_special_release(query: &AlbumQuery) -> bool {
    is_placeholder_artist(&query.album_artist, None)
        || normalize_group_key(&query.album)
            .split(|character: char| !character.is_alphanumeric())
            .any(|word| matches!(word, "live" | "remix"))
}

fn same_group_key(left: &str, right: &str) -> bool {
    let left = normalize_group_key(left);
    !left.is_empty() && left == normalize_group_key(right)
}
