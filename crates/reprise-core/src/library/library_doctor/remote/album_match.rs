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

/// The score a candidate must reach before it counts as the album's release.
///
/// The four signals are worth 40 (track count), 30 (title overlap), 20 (artist
/// credit) and 10 (year), so the floor is set where two of them have to agree:
/// a matching track count plus a third of the titles, or the full tracklist
/// plus the artist credit. No single signal carries a match on its own —
/// track counts and generic album titles collide across unrelated releases,
/// and a search that returns only such collisions has found nothing. Being
/// the best of a bad field is not a match.
pub(crate) const MINIMUM_RELEASE_SCORE: u8 = 50;

pub(crate) fn best_release(
    query: &AlbumQuery,
    candidates: &[RemoteIdentity],
) -> Option<AlbumMatch> {
    candidates
        .iter()
        .map(|identity| score_release(query, identity))
        .min_by(compare_matches)
        .filter(|matched| matched.score >= MINIMUM_RELEASE_SCORE)
}

pub(crate) fn joint_confidence(release_score: u8, field_agreement: u8) -> u8 {
    let product = u16::from(release_score) * u16::from(field_agreement);
    u8::try_from((product + 50) / 100).unwrap_or(100)
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

/// Words that name one of the demoted release kinds, in the order of
/// `is_penalized_release`: compilation, DJ-mix, live, mixtape, remix.
const SPECIAL_RELEASE_WORDS: [&str; 12] = [
    "compilation",
    "compilations",
    "anthology",
    "sampler",
    "hits",
    "megamix",
    "live",
    "unplugged",
    "concert",
    "mixtape",
    "remix",
    "remixes",
];

/// Two-word names for the same kinds. They cannot be recognised word by word:
/// "mix" and "best" on their own say nothing.
const SPECIAL_RELEASE_PHRASES: [&str; 4] = ["best of", "dj mix", "dj set", "mixed by"];

/// The demotion covers five secondary types, so this exception has to speak
/// about all five — a correctly tagged DJ mix must not be punished for being
/// one.
///
/// Covered: a placeholder album artist, and an album title that names its own
/// kind (`SPECIAL_RELEASE_WORDS`, `SPECIAL_RELEASE_PHRASES`).
///
/// Deliberately not covered: the genre. `AlbumQuery` carries the album artist,
/// the album, its track titles, the track count and the year — there is no
/// genre in it, and inventing one here would be a claim the data cannot back.
/// A DJ mix, live album or compilation whose title says nothing about itself
/// therefore stays demoted; the local tags simply do not say otherwise.
fn local_tags_describe_special_release(query: &AlbumQuery) -> bool {
    let album = normalize_group_key(&query.album);
    is_placeholder_artist(&query.album_artist, None)
        || album
            .split(|character: char| !character.is_alphanumeric())
            .any(|word| SPECIAL_RELEASE_WORDS.contains(&word))
        || SPECIAL_RELEASE_PHRASES
            .iter()
            .any(|phrase| album.contains(phrase))
}

fn same_group_key(left: &str, right: &str) -> bool {
    let left = normalize_group_key(left);
    !left.is_empty() && left == normalize_group_key(right)
}

#[cfg(test)]
mod confidence_tests {
    #[test]
    fn joint_confidence_multiplies_release_and_field_scores() {
        assert_eq!(super::joint_confidence(80, 75), 60);
    }

    #[test]
    fn joint_confidence_rounds_to_the_nearest_percent() {
        assert_eq!(super::joint_confidence(99, 100), 99);
    }
}
