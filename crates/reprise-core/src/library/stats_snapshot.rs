use std::collections::{BTreeMap, BTreeSet};

use chrono::{Datelike, NaiveDate, TimeZone};
use rusqlite::Connection;

use super::group_key::KeyResolver;
use super::stats_period::{
    apply_activity_granularity, local_parts, week_start, PeriodRange, StatsPeriod,
};
use super::stats_screen::{
    album_rows, artist_rows, first_event_unix, fold_album_rows, genre_artist_rows, genre_rows,
    key_resolver, listen_rows, ranked_groups, total_ms_in_range, track_rows, GenreArtistRow,
    RankedGroup, TopAlbum, TopTrack, TrackAggregate,
};

const SPOTLIGHT_TRACK_LIMIT: usize = 3;
const SPOTLIGHT_ALSO_LIMIT: usize = 4;
const GENRE_LIMIT: usize = 5;
/// The first rounded upward percentage that switches to multiplicative copy.
pub const COMPARISON_FACTOR_PERCENT_THRESHOLD: i64 = 1_000;
/// A decline at or beyond this rounded percentage reads more clearly as a factor.
pub const COMPARISON_FACTOR_DECLINE_PERCENT_THRESHOLD: i64 = -50;
/// Baselines below the UI's one-minute display granularity are qualitative.
pub const COMPARISON_EFFECTIVELY_ZERO_MS: i64 = 60_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SortBy {
    Plays,
    Time,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComparisonDirection {
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComparisonFactor {
    Whole(u64),
    Decimal { whole: u64, tenth: u8 },
    LessThanOneTenth,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComparisonPresentation {
    Percentage(i64),
    Factor {
        direction: ComparisonDirection,
        value: ComparisonFactor,
    },
    New,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HeroSection {
    pub total_ms: i64,
    pub plays: i64,
    pub average_ms_per_day: i64,
    pub artists: i64,
    pub previous_ms: Option<i64>,
    pub pace_projection_ms: Option<i64>,
    pub comparison_percent: Option<i64>,
    pub comparison_presentation: Option<ComparisonPresentation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RibbonPoint {
    pub label: String,
    pub total_ms: i64,
    pub open: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpotlightSection {
    pub artist: RankedGroup,
    pub share_percent: i64,
    pub top_tracks: Vec<TopTrack>,
    pub also: Vec<RankedGroup>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenreSegment {
    pub label: String,
    pub key: String,
    pub plays: i64,
    pub total_ms: i64,
    pub share_percent: i64,
    pub variant_count: usize,
    pub top_artist: Option<String>,
    pub representative_track_path: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GenreSection {
    pub segments: Vec<GenreSegment>,
    pub denominator_ms: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BestWeek {
    pub start: NaiveDate,
    pub total_ms: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatsSnapshot {
    pub period: PeriodRange,
    pub hero: HeroSection,
    pub ribbon: Vec<RibbonPoint>,
    pub best_week: Option<BestWeek>,
    pub spotlight: Option<SpotlightSection>,
    pub genres: GenreSection,
    pub top_artists: Vec<RankedGroup>,
    pub top_albums: Vec<TopAlbum>,
    pub top_tracks: Vec<TopTrack>,
}

impl StatsSnapshot {
    pub fn is_empty(&self) -> bool {
        self.hero.plays == 0
    }

    pub fn top_tracks_sorted(&self, sort_by: SortBy) -> Vec<TopTrack> {
        let mut tracks = self.top_tracks.clone();
        sort_tracks(&mut tracks, sort_by);
        tracks
    }
}

/// Computes one owned, side-effect-free snapshot from the selected period.
pub fn compute<Tz: TimeZone>(
    conn: &Connection,
    period: StatsPeriod,
    now_unix: i64,
    tz: &Tz,
) -> Result<StatsSnapshot, rusqlite::Error> {
    // Eight read statements in a stable order. Keeping this function pure is
    // the seam that permits a transparent cache wrapper later if profiling
    // ever justifies one.
    let first_event = first_event_unix(conn)?; // 1
    let mut range = period.resolve(now_unix, tz, first_event);
    let listen_rows = listen_rows(conn, range.start_unix, range.end_unix)?; // 2

    // The compared span is seasonally congruent, not merely the equally long
    // stretch immediately before — see [`StatsPeriod::previous_range`]. A
    // period without one stays distinct from a real zero baseline: All time
    // has no comparison, while an empty compared span has qualitative value.
    let previous_ms = match period.previous_range(now_unix, tz) {
        Some((start, end)) => Some(total_ms_in_range(conn, start, end)?), // 3
        None => None,
    };
    let artists = artist_rows(conn, range.start_unix, range.end_unix)?; // 4
    let albums = album_rows(conn, range.start_unix, range.end_unix)?; // 5
    let genres = genre_rows(conn, range.start_unix, range.end_unix)?; // 6
    let genre_artists = genre_artist_rows(conn, range.start_unix, range.end_unix)?; // 7
    let track_aggregates = track_rows(conn, range.start_unix, range.end_unix)?; // 8

    if listen_rows.is_empty() {
        range.buckets.clear();
    } else {
        let active_days = listen_rows
            .iter()
            .filter_map(|row| local_parts(tz, row.played_at).map(|(day, _)| day))
            .collect::<BTreeSet<_>>();
        apply_activity_granularity(&mut range, tz, active_days.len() as i64);
    }

    let total_ms = listen_rows.iter().map(|row| row.ms).sum::<i64>();
    let top_artists = ranked_groups(&artists);
    let top_albums = fold_album_rows(&albums);
    let mut top_tracks = track_aggregates
        .iter()
        .map(|row| row.track.clone())
        .collect::<Vec<_>>();
    sort_tracks(&mut top_tracks, SortBy::Plays);

    let comparison_percent = previous_ms
        .filter(|value| *value >= COMPARISON_EFFECTIVELY_ZERO_MS)
        .and_then(|value| comparison_percent(total_ms, value));
    let elapsed_days = elapsed_days(tz, &range).max(1);
    let pace_projection_ms = match period {
        StatsPeriod::YearToDate(year)
            if tz
                .timestamp_opt(now_unix, 0)
                .earliest()
                .is_some_and(|now| now.year() == year) =>
        {
            days_in_year(year).map(|days| (total_ms / elapsed_days).saturating_mul(days))
        }
        _ => None,
    };
    let hero = HeroSection {
        total_ms,
        plays: listen_rows.len() as i64,
        average_ms_per_day: total_ms / elapsed_days,
        artists: top_artists.len() as i64,
        previous_ms,
        pace_projection_ms,
        comparison_percent,
        comparison_presentation: previous_ms
            .and_then(|value| comparison_presentation(total_ms, value, comparison_percent)),
    };
    let ribbon = range
        .buckets
        .iter()
        .map(|bucket| RibbonPoint {
            label: bucket.label.clone(),
            total_ms: listen_rows
                .iter()
                .filter(|row| row.played_at >= bucket.start_unix && row.played_at < bucket.end_unix)
                .map(|row| row.ms)
                .sum(),
            open: bucket.open,
        })
        .collect();
    let best_week = best_week(&listen_rows, tz);
    let spotlight = spotlight(&top_artists, &key_resolver(&artists), &track_aggregates);
    let genres = genre_section(&genres, &genre_artists);

    Ok(StatsSnapshot {
        period: range,
        hero,
        ribbon,
        best_week,
        spotlight,
        genres,
        top_artists,
        top_albums,
        top_tracks,
    })
}

fn best_week<Tz: TimeZone>(rows: &[super::stats_screen::ListenRow], tz: &Tz) -> Option<BestWeek> {
    let mut totals = BTreeMap::<NaiveDate, i64>::new();
    for row in rows {
        let Some(start) = week_start(tz, row.played_at) else {
            continue;
        };
        *totals.entry(start).or_default() += row.ms;
    }
    totals
        .into_iter()
        .max_by(|(left_start, left_ms), (right_start, right_ms)| {
            left_ms
                .cmp(right_ms)
                .then_with(|| right_start.cmp(left_start))
        })
        .map(|(start, total_ms)| BestWeek { start, total_ms })
}

fn comparison_percent(current_ms: i64, previous_ms: i64) -> Option<i64> {
    if previous_ms <= 0 {
        return None;
    }
    Some((((current_ms - previous_ms) as f64 / previous_ms as f64) * 100.0).round() as i64)
}

fn comparison_presentation(
    current_ms: i64,
    previous_ms: i64,
    percent: Option<i64>,
) -> Option<ComparisonPresentation> {
    if current_ms <= 0 {
        return None;
    }
    if previous_ms < COMPARISON_EFFECTIVELY_ZERO_MS {
        return Some(ComparisonPresentation::New);
    }
    let percent = percent?;
    let direction = if percent >= COMPARISON_FACTOR_PERCENT_THRESHOLD {
        ComparisonDirection::Up
    } else if percent <= COMPARISON_FACTOR_DECLINE_PERCENT_THRESHOLD {
        ComparisonDirection::Down
    } else {
        return Some(ComparisonPresentation::Percentage(percent));
    };
    let tenths = ((current_ms as f64 / previous_ms as f64) * 10.0).round() as u64;
    let value = if tenths == 0 {
        ComparisonFactor::LessThanOneTenth
    } else if tenths.is_multiple_of(10) {
        ComparisonFactor::Whole(tenths / 10)
    } else {
        ComparisonFactor::Decimal {
            whole: tenths / 10,
            tenth: (tenths % 10) as u8,
        }
    };
    Some(ComparisonPresentation::Factor { direction, value })
}

/// `keys` must be the resolver built from the very rows `artists` was folded
/// from. Keying a track through anything else re-creates the split STATS-9
/// removes: a track without an MBID would miss its own artist's group.
///
/// `share_percent` divides by the artist population, matching
/// [`genre_section`]: every section states a share of its own categorized
/// total, never of a wider one that includes rows the section cannot show.
fn spotlight(
    artists: &[RankedGroup],
    keys: &KeyResolver,
    tracks: &[TrackAggregate],
) -> Option<SpotlightSection> {
    let denominator_ms = artists.iter().map(|artist| artist.group.ms).sum::<i64>();
    let artist = artists.first()?.clone();
    let mut artist_tracks = tracks
        .iter()
        .filter(|track| keys.key_for(&track.effective_artist) == artist.group.key)
        .map(|track| track.track.clone())
        .collect::<Vec<_>>();
    sort_tracks(&mut artist_tracks, SortBy::Plays);
    artist_tracks.truncate(SPOTLIGHT_TRACK_LIMIT);
    Some(SpotlightSection {
        share_percent: percent(artist.group.ms, denominator_ms),
        artist,
        top_tracks: artist_tracks,
        also: artists
            .iter()
            .skip(1)
            .take(SPOTLIGHT_ALSO_LIMIT)
            .cloned()
            .collect(),
    })
}

/// Shares divide by the genre population, not by total listening: tracks
/// without a genre are neither a segment nor "Other" (STATS-3), so counting
/// them in the denominator would make the bar add up to less than 100 %.
/// The spotlight follows the same rule against the artist population.
fn genre_section(
    rows: &[super::stats_screen::NamedRow],
    genre_artists: &[GenreArtistRow],
) -> GenreSection {
    let groups = ranked_groups(rows);
    let resolver = key_resolver(rows);
    let denominator_ms = groups.iter().map(|row| row.group.ms).sum::<i64>();
    let mut segments = groups
        .iter()
        .take(GENRE_LIMIT)
        .map(|row| {
            let (top_artist, representative_track_path) =
                genre_tile_metadata(&resolver, &row.group.key, genre_artists);
            GenreSegment {
                label: row.group.label.clone(),
                key: row.group.key.clone(),
                plays: row.group.plays,
                total_ms: row.group.ms,
                share_percent: percent(row.group.ms, denominator_ms),
                variant_count: row.group.variant_count,
                top_artist,
                representative_track_path,
            }
        })
        .collect::<Vec<_>>();
    if groups.len() > GENRE_LIMIT {
        let rest = &groups[GENRE_LIMIT..];
        let plays = rest.iter().map(|row| row.group.plays).sum();
        let total_ms = rest.iter().map(|row| row.group.ms).sum();
        segments.push(GenreSegment {
            label: "Other".to_string(),
            key: "other".to_string(),
            plays,
            total_ms,
            share_percent: percent(total_ms, denominator_ms),
            variant_count: 1,
            top_artist: None,
            representative_track_path: String::new(),
        });
    }
    GenreSection {
        segments,
        denominator_ms,
    }
}

fn genre_tile_metadata(
    genre_resolver: &KeyResolver,
    genre_key: &str,
    rows: &[GenreArtistRow],
) -> (Option<String>, String) {
    let matching = rows
        .iter()
        .filter(|row| genre_resolver.key_for(&row.genre_raw) == genre_key)
        .collect::<Vec<_>>();
    let artists = matching
        .iter()
        .map(|row| row.artist.clone())
        .collect::<Vec<_>>();
    let top_artist = ranked_groups(&artists)
        .into_iter()
        .next()
        .map(|artist| artist.group.label);
    let representative_track_path = matching
        .into_iter()
        .max_by(|left, right| {
            left.artist
                .ms
                .cmp(&right.artist.ms)
                .then_with(|| left.artist.plays.cmp(&right.artist.plays))
                .then_with(|| left.artist.last_played_at.cmp(&right.artist.last_played_at))
                .then_with(|| right.artist.path.cmp(&left.artist.path))
        })
        .map(|row| row.artist.path.clone())
        .unwrap_or_default();
    (top_artist, representative_track_path)
}

fn sort_tracks(tracks: &mut [TopTrack], sort_by: SortBy) {
    tracks.sort_by(|left, right| {
        let primary = match sort_by {
            SortBy::Plays => right.play_count.cmp(&left.play_count),
            SortBy::Time => right.total_ms.cmp(&left.total_ms),
        };
        primary
            .then_with(|| right.total_ms.cmp(&left.total_ms))
            .then_with(|| right.play_count.cmp(&left.play_count))
            .then_with(|| left.title.cmp(&right.title))
            .then_with(|| left.track_id.cmp(&right.track_id))
    });
}

fn elapsed_days<Tz: TimeZone>(tz: &Tz, range: &PeriodRange) -> i64 {
    let Some((start, _)) = local_parts(tz, range.start_unix) else {
        return 0;
    };
    let Some((end, _)) = local_parts(tz, range.end_unix.saturating_sub(1)) else {
        return 0;
    };
    (end - start).num_days().saturating_add(1)
}

fn days_in_year(year: i32) -> Option<i64> {
    let start = NaiveDate::from_ymd_opt(year, 1, 1)?;
    let end = NaiveDate::from_ymd_opt(year.checked_add(1)?, 1, 1)?;
    Some((end - start).num_days())
}

fn percent(value: i64, total: i64) -> i64 {
    if total <= 0 {
        0
    } else {
        ((value as f64 / total as f64) * 100.0).round() as i64
    }
}

#[cfg(test)]
#[path = "stats_snapshot_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "stats_comparison_tests.rs"]
mod comparison_tests;

#[cfg(test)]
#[path = "stats_snapshot_hero_tests.rs"]
mod hero_tests;

#[cfg(test)]
#[path = "stats_snapshot_genre_tests.rs"]
mod genre_tests;
