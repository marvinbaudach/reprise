//! Pure podcast row formatting, filtering, and sorting.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};
use reprise_core::format::DatePattern;
use reprise_core::podcasts::download_state::DownloadState;
use reprise_core::podcasts::{EpisodeRow, EpisodeStatus, PodcastKind, SourceGroup};
use reprise_view::search_scope;

use crate::ui::enumerated::enumerated;
use crate::ui::strings;

/// The filter the podcast view applies: the three facets the core persists,
/// plus the section's transient search query.
///
/// The facets are named exactly as `PodcastFilterConfig` names them and go
/// back to it through [`PodcastFilter::facets`], so the round trip through the
/// database still has one spelling. `query` deliberately stays out of that
/// round trip — SEARCH-8a makes the query belong to the visit, not to the
/// saved view; persisting it would resurrect a search the user never typed
/// again on the next launch.
///
/// (`SRC-10` addendum, Block B2: `downloaded_only` is the "Downloaded" chip —
/// it matches only episodes with a file on disk right now, not a queued or
/// downloading one.)
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct PodcastFilter {
    pub unplayed_only: bool,
    pub source: Option<PodcastKind>,
    pub downloaded_only: bool,
    /// `POD-25`: matched case-insensitively against episode titles alone.
    pub query: String,
}

impl PodcastFilter {
    /// The persisted half. Named rather than derived through a `From` so the
    /// call sites that write to the database read as "facets only".
    pub(super) fn facets(&self) -> PodcastFilterConfig {
        PodcastFilterConfig {
            unplayed_only: self.unplayed_only,
            source: self.source,
            downloaded_only: self.downloaded_only,
        }
    }

    /// Restores the persisted half, leaving the query empty — a launch never
    /// starts inside somebody's old search.
    pub(super) fn from_facets(facets: &PodcastFilterConfig) -> Self {
        Self {
            unplayed_only: facets.unplayed_only,
            source: facets.source,
            downloaded_only: facets.downloaded_only,
            query: String::new(),
        }
    }

    pub(super) fn with_query(&self, query: &str) -> Self {
        Self {
            query: query.trim().to_owned(),
            ..self.clone()
        }
    }

    pub(super) fn has_query(&self) -> bool {
        auto_expand_for_query(&self.query)
    }
}

pub(super) use reprise_core::podcasts::config::PodcastFilterConfig;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Pill {
    pub label: &'static str,
    pub icon: Option<&'static str>,
    pub css_class: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SourceSummary {
    pub episode_count: usize,
    pub new_count: usize,
    pub downloaded_bytes: i64,
    pub latest_published_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RenderedSourceGroup {
    pub group: SourceGroup,
    pub summary: SourceSummary,
}

/// `G2` (design 6a): the page-level header line above the grouped list
/// ("4 shows · 41 episodes · 7 new").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct LibrarySummary {
    pub shows: usize,
    pub episodes: usize,
    pub new: usize,
}

/// `G2`: a pure projection over the **unfiltered** group set — always the
/// whole library, independent of the active filter, so the header keeps
/// reading as an overview rather than jittering with every filter chip.
/// "new" uses the same discovery-time definition as the per-group facts
/// line (`SourceSummary::new_count`) so playback never rewrites discovery
/// history and both counts stay aligned.
pub(super) fn library_summary(groups: &[SourceGroup]) -> LibrarySummary {
    let mut episodes = 0_usize;
    let mut new = 0_usize;
    for group in groups {
        episodes += group.episodes.len();
        new += group
            .episodes
            .iter()
            .filter(|episode| episode.is_new)
            .count();
    }
    LibrarySummary {
        shows: groups.len(),
        episodes,
        new,
    }
}

pub(super) fn source_summary(
    group: &SourceGroup,
    download_states: &BTreeMap<i64, DownloadState>,
) -> SourceSummary {
    SourceSummary {
        episode_count: group.episodes.len(),
        new_count: group
            .episodes
            .iter()
            .filter(|episode| episode.is_new)
            .count(),
        downloaded_bytes: group
            .episodes
            .iter()
            .filter_map(|episode| match download_states.get(&episode.id) {
                Some(DownloadState::Downloaded { bytes }) => {
                    Some((*bytes).try_into().unwrap_or(i64::MAX))
                }
                _ => None,
            })
            .fold(0_i64, i64::saturating_add),
        latest_published_at: group
            .episodes
            .iter()
            .filter_map(|episode| episode.published_at)
            .max(),
    }
}

pub(super) fn rendered_source_groups(
    groups: &[SourceGroup],
    filter: &PodcastFilter,
    download_states: &BTreeMap<i64, DownloadState>,
) -> Vec<RenderedSourceGroup> {
    groups
        .iter()
        .filter_map(|group| {
            let episodes = apply_filter(&group.episodes, filter);
            if episodes.is_empty() && active(filter) {
                return None;
            }
            let summary = source_summary(group, download_states);
            let mut rendered = group.clone();
            rendered.episodes = episodes;
            Some(RenderedSourceGroup {
                group: rendered,
                summary,
            })
        })
        .collect()
}

pub(super) fn relative_date(timestamp: Option<i64>, today: NaiveDate) -> String {
    relative_date_with(timestamp, today, &crate::ui::date_format::current().date)
}

pub(super) fn relative_date_with(
    timestamp: Option<i64>,
    today: NaiveDate,
    pattern: &DatePattern,
) -> String {
    let Some(date) = timestamp
        .and_then(|value| DateTime::<Utc>::from_timestamp(value, 0))
        .map(|value| value.with_timezone(&Local).date_naive())
    else {
        return String::new();
    };
    if date == today {
        strings::text(strings::PODCAST_TODAY)
    } else if date.succ_opt() == Some(today) {
        strings::text(strings::PODCAST_YESTERDAY)
    } else {
        pattern.render(Some(date.year()), Some(date.month()), Some(date.day()))
    }
}

pub(super) fn duration(duration_secs: Option<i64>) -> String {
    let Some(seconds) = duration_secs.filter(|seconds| *seconds >= 0) else {
        return String::new();
    };
    if seconds < 60 {
        strings::text(strings::PODCAST_DURATION_UNDER_MINUTE)
    } else if seconds < 3_600 {
        strings::podcast_duration_minutes(seconds / 60)
    } else {
        strings::podcast_duration_hours(seconds / 3_600, (seconds % 3_600) / 60)
    }
}

pub(super) fn file_size(bytes: Option<i64>) -> Option<String> {
    let bytes = bytes.filter(|bytes| *bytes > 0)?;
    let bytes = bytes as f64;
    const MIB: f64 = 1_048_576.0;
    const GIB: f64 = 1_073_741_824.0;
    if bytes >= GIB {
        Some(format!("{:.1} GB", bytes / GIB))
    } else {
        Some(format!("{:.1} MB", bytes / MIB))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SourceHeader<'a> {
    pub(super) title: &'a str,
    pub(super) subtitle: Option<&'a str>,
}

pub(super) fn source_header<'a>(
    kind: PodcastKind,
    title: &'a str,
    author: Option<&'a str>,
) -> SourceHeader<'a> {
    SourceHeader {
        title,
        subtitle: match kind {
            PodcastKind::Rss => author_line(title, author),
            PodcastKind::Youtube => None,
        },
    }
}

pub(super) fn author_line<'a>(title: &str, author: Option<&'a str>) -> Option<&'a str> {
    let author = author.map(str::trim).filter(|author| !author.is_empty())?;
    let normalized_title = title.trim().to_lowercase();
    let normalized_author = author.to_lowercase();
    if normalized_title == normalized_author {
        return None;
    }
    if let Some(remainder) = normalized_title.strip_prefix(&normalized_author) {
        if remainder
            .chars()
            .next()
            .is_some_and(|character| !character.is_alphanumeric())
        {
            return None;
        }
    }
    Some(author)
}

pub(super) fn source_pill(kind: PodcastKind) -> Pill {
    match kind {
        PodcastKind::Rss => Pill {
            label: strings::PODCAST_SOURCE_RSS,
            icon: Some("application-rss+xml-symbolic"),
            css_class: "reprise-podcast-source",
        },
        PodcastKind::Youtube => Pill {
            label: strings::PODCAST_SOURCE_YOUTUBE,
            icon: Some("video-x-generic-symbolic"),
            css_class: "reprise-podcast-source",
        },
    }
}

pub(super) fn status_pill(row: &EpisodeRow) -> Option<Pill> {
    match reprise_core::podcasts::status::derive(row.played_at, row.position_ms) {
        EpisodeStatus::New if row.is_new => Some(Pill {
            label: strings::PODCAST_STATUS_NEW,
            icon: None,
            css_class: "reprise-podcast-status-new",
        }),
        EpisodeStatus::New => None,
        EpisodeStatus::Resume => Some(Pill {
            label: strings::PODCAST_STATUS_RESUME,
            icon: None,
            css_class: "reprise-podcast-status-resume",
        }),
        EpisodeStatus::Played => Some(Pill {
            label: strings::PODCAST_STATUS_PLAYED,
            icon: None,
            css_class: "reprise-podcast-status-played",
        }),
    }
}

/// `POD-25`: the query reads episode titles and nothing else — not the show,
/// not the author, not the description. The chip says "in episode titles"
/// (FIL-1d), and this is the function that has to keep that promise true.
pub(super) fn matches_filter(row: &EpisodeRow, filter: &PodcastFilter) -> bool {
    (!filter.unplayed_only || row.played_at.is_none())
        && filter.source.is_none_or(|source| row.kind == source)
        && (!filter.downloaded_only || row.downloaded_path.is_some())
        && search_scope::matches_query(&row.title, &filter.query)
}

pub(super) fn apply_filter(rows: &[EpisodeRow], filter: &PodcastFilter) -> Vec<EpisodeRow> {
    rows.iter()
        .filter(|row| matches_filter(row, filter))
        .cloned()
        .collect()
}

enumerated! {
    /// Declaration order is the tie-break order: `PODCAST_FACETS` lists the
    /// facets in it, and `Ord` follows it, so two equally cheap relaxations
    /// always resolve the same way.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub(super) enum PodcastFilterFacet {
        Unplayed,
        Source,
        Downloaded,
        /// `POD-25`: the search chip is a facet like the others, so a jump to
        /// an episode the query hides relaxes it instead of landing on an
        /// empty page. It comes last because dropping the user's typed query
        /// is the most surprising relaxation of the four.
        Query,
    }

    /// Generated from the declaration above: a facet that is missing here is
    /// never relaxed, so a jump into a filtered view silently does nothing.
    pub(super) const PODCAST_FACETS;
}

/// A filter containing only the selected facet. This deliberately delegates
/// the facet's actual matching semantics back to `matches_filter`.
fn only_facet(filter: &PodcastFilter, facet: PodcastFilterFacet) -> PodcastFilter {
    match facet {
        PodcastFilterFacet::Unplayed => PodcastFilter {
            unplayed_only: filter.unplayed_only,
            ..PodcastFilter::default()
        },
        PodcastFilterFacet::Source => PodcastFilter {
            source: filter.source,
            ..PodcastFilter::default()
        },
        PodcastFilterFacet::Downloaded => PodcastFilter {
            downloaded_only: filter.downloaded_only,
            ..PodcastFilter::default()
        },
        PodcastFilterFacet::Query => PodcastFilter {
            query: filter.query.clone(),
            ..PodcastFilter::default()
        },
    }
}

/// `matches_filter` is a conjunction of independent facets, so a facet hides
/// the row exactly when the row fails that facet alone.
pub(super) fn facet_hides(
    row: &EpisodeRow,
    filter: &PodcastFilter,
    facet: PodcastFilterFacet,
) -> bool {
    !matches_filter(row, &only_facet(filter, facet))
}

pub(super) fn remove_facet(filter: &PodcastFilter, facet: PodcastFilterFacet) -> PodcastFilter {
    let mut result = filter.clone();
    match facet {
        PodcastFilterFacet::Unplayed => result.unplayed_only = false,
        PodcastFilterFacet::Source => result.source = None,
        PodcastFilterFacet::Downloaded => result.downloaded_only = false,
        PodcastFilterFacet::Query => result.query.clear(),
    }
    result
}

/// The facets that hide `row`, in `PODCAST_FACETS` order — the smallest
/// relaxation that makes this one row visible, since `matches_filter` is a
/// conjunction of independent facets.
fn hiding_facets(row: &EpisodeRow, filter: &PodcastFilter) -> Vec<PodcastFilterFacet> {
    PODCAST_FACETS
        .into_iter()
        .filter(|facet| facet_hides(row, filter, *facet))
        .collect()
}

fn without_facets(filter: &PodcastFilter, facets: &[PodcastFilterFacet]) -> PodcastFilter {
    facets.iter().fold(filter.clone(), |filter, facet| {
        remove_facet(&filter, *facet)
    })
}

/// `SRC-13`: the filter an explicit jump to this episode needs — unchanged
/// when the episode is visible, otherwise the same filter minus exactly the
/// facets that hide it.
pub(super) fn filter_without_hiding(row: &EpisodeRow, filter: &PodcastFilter) -> PodcastFilter {
    if matches_filter(row, filter) {
        return filter.clone();
    }
    without_facets(filter, &hiding_facets(row, filter))
}

/// The filter required for a channel jump. A filtered group disappears when no
/// episode survives, so the group is back as soon as *one* episode is visible
/// again — dropping only the facets that every episode fails is not enough,
/// because different episodes can fail different facets and then no single
/// facet fails for all of them.
///
/// So take the cheapest episode's own relaxation ([`filter_without_hiding`]'s
/// facet set), which is minimal for that episode and therefore minimal for the
/// group: every relaxation that revives the group revives some episode, and
/// that episode's hiding facets are a subset of it. Ties resolve by
/// `PODCAST_FACETS` order, so the result does not depend on episode order.
pub(super) fn filter_without_hiding_group(
    group: &SourceGroup,
    filter: &PodcastFilter,
) -> PodcastFilter {
    if !apply_filter(&group.episodes, filter).is_empty() {
        return filter.clone();
    }
    group
        .episodes
        .iter()
        .map(|row| hiding_facets(row, filter))
        .min_by(|left, right| left.len().cmp(&right.len()).then_with(|| left.cmp(right)))
        .map_or_else(|| filter.clone(), |facets| without_facets(filter, &facets))
}

pub(super) fn active(filter: &PodcastFilter) -> bool {
    filter.unplayed_only || filter.downloaded_only || filter.has_query()
}

/// `POD-25`: a query is a promise that the matches are on screen, so every
/// show that survived it opens itself. Manual expansion state is untouched —
/// the renderer forces the expanders open for this render pass only, and
/// removing the query hands each show back its own collapsed/expanded state.
///
/// Takes the query rather than the whole filter because the renderer only
/// ever carries the query: this stays the one definition of "a query is
/// active", read by `PodcastFilter::has_query` and by `podcasts_groups`
/// alike, instead of the same `trim().is_empty()` living in both.
pub(super) fn auto_expand_for_query(query: &str) -> bool {
    !query.trim().is_empty()
}

pub(super) fn sort_newest_first(rows: &mut [EpisodeRow]) {
    rows.sort_by(
        |left, right| match (left.published_at, right.published_at) {
            (Some(left_date), Some(right_date)) => right_date
                .cmp(&left_date)
                .then_with(|| right.id.cmp(&left.id)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => right.first_seen_at.cmp(&left.first_seen_at),
        },
    );
}

pub(super) fn updated_ago(timestamp: Option<i64>, now: i64) -> String {
    let Some(timestamp) = timestamp else {
        return strings::text(strings::PODCAST_UPDATED_JUST_NOW);
    };
    let minutes = now.saturating_sub(timestamp).max(0) / 60;
    if minutes == 0 {
        strings::text(strings::PODCAST_UPDATED_JUST_NOW)
    } else {
        strings::podcast_updated_minutes_ago(minutes)
    }
}

#[cfg(test)]
#[path = "podcasts_presentation_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "podcasts_presentation_filter_tests.rs"]
mod filter_tests;
