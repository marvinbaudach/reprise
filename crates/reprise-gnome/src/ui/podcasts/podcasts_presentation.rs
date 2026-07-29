//! Pure podcast row formatting, filtering, and sorting.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};
use reprise_core::podcasts::download_state::DownloadState;
use reprise_core::podcasts::{EpisodeRow, EpisodeStatus, PodcastKind, SourceGroup};

use crate::ui::strings;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct PodcastFilter {
    pub unplayed_only: bool,
    pub show: Option<String>,
    pub source: Option<PodcastKind>,
    /// `SRC-10` addendum (Block B2): the "Downloaded" chip — matches only
    /// episodes with a file on disk right now, not a queued/downloading one.
    pub downloaded_only: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Pill {
    pub label: &'static str,
    pub icon: Option<&'static str>,
    pub css_class: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SourceSummary {
    pub episode_count: usize,
    pub unplayed_count: usize,
    pub downloaded_bytes: i64,
    pub latest_published_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RenderedSourceGroup {
    pub group: SourceGroup,
    pub summary: SourceSummary,
}

pub(super) fn source_summary(
    group: &SourceGroup,
    download_states: &BTreeMap<i64, DownloadState>,
) -> SourceSummary {
    SourceSummary {
        episode_count: group.episodes.len(),
        unplayed_count: group
            .episodes
            .iter()
            .filter(|episode| episode.played_at.is_none())
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
    let Some(date) = timestamp
        .and_then(|value| DateTime::<Utc>::from_timestamp(value, 0))
        .map(|value| value.with_timezone(&Local).date_naive())
    else {
        return "—".to_owned();
    };
    if date == today {
        strings::text(strings::PODCAST_TODAY)
    } else if date.succ_opt() == Some(today) {
        strings::text(strings::PODCAST_YESTERDAY)
    } else if date.year() == today.year() {
        date.format("%-d. %b").to_string()
    } else {
        date.format("%-d. %b %Y").to_string()
    }
}

pub(super) fn duration(duration_secs: Option<i64>) -> String {
    duration_secs.map_or_else(
        || "—".to_owned(),
        |seconds| {
            let seconds = seconds.max(0);
            format!("{}:{:02}", seconds / 3_600, (seconds % 3_600) / 60)
        },
    )
}

pub(super) fn file_size(bytes: Option<i64>) -> String {
    let Some(bytes) = bytes.filter(|bytes| *bytes >= 0) else {
        return "—".to_owned();
    };
    let bytes = bytes as f64;
    const MIB: f64 = 1_048_576.0;
    const GIB: f64 = 1_073_741_824.0;
    if bytes >= GIB {
        format!("{:.1} GB", bytes / GIB)
    } else {
        format!("{:.1} MB", bytes / MIB)
    }
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

pub(super) fn status_pill(row: &EpisodeRow) -> Pill {
    match reprise_core::podcasts::status::derive(row.played_at, row.position_ms) {
        EpisodeStatus::New => Pill {
            label: strings::PODCAST_STATUS_NEW,
            icon: None,
            css_class: "reprise-podcast-status-new",
        },
        EpisodeStatus::Resume => Pill {
            label: strings::PODCAST_STATUS_RESUME,
            icon: None,
            css_class: "reprise-podcast-status-resume",
        },
        EpisodeStatus::Played => Pill {
            label: strings::PODCAST_STATUS_PLAYED,
            icon: None,
            css_class: "reprise-podcast-status-played",
        },
    }
}

pub(super) fn matches_filter(row: &EpisodeRow, filter: &PodcastFilter) -> bool {
    (!filter.unplayed_only || row.played_at.is_none())
        && filter.show.as_deref().is_none_or(|show| row.show == show)
        && filter.source.is_none_or(|source| row.kind == source)
        && (!filter.downloaded_only || row.downloaded_path.is_some())
}

pub(super) fn apply_filter(rows: &[EpisodeRow], filter: &PodcastFilter) -> Vec<EpisodeRow> {
    rows.iter()
        .filter(|row| matches_filter(row, filter))
        .cloned()
        .collect()
}

pub(super) fn active(filter: &PodcastFilter) -> bool {
    filter.unplayed_only
        || filter.show.is_some()
        || filter.source.is_some()
        || filter.downloaded_only
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
mod tests {
    use super::*;

    fn row(id: i64, published_at: Option<i64>, kind: PodcastKind) -> EpisodeRow {
        EpisodeRow {
            id,
            subscription_id: 1,
            guid: format!("g{id}"),
            title: format!("Episode {id}"),
            show: if id == 3 { "Other" } else { "Show" }.into(),
            show_image_url: None,
            kind,
            audio_url: "https://example.test/episode.mp3".into(),
            page_url: None,
            published_at,
            duration_secs: Some(4_533),
            downloaded_path: None,
            downloaded_bytes: None,
            played_at: None,
            position_ms: 0,
            first_seen_at: id,
        }
    }

    #[test]
    fn pod_9_presentation_formats_date_length_source_and_status() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 26).unwrap();
        let today_timestamp = today.and_hms_opt(12, 0, 0).unwrap().and_utc().timestamp();
        assert_eq!(relative_date(Some(today_timestamp), today), "Today");
        assert_eq!(
            relative_date(Some(today_timestamp - 86_400), today),
            "Yesterday"
        );
        assert_eq!(duration(Some(4_533)), "1:15");
        assert_eq!(file_size(Some(41_943_040)), "40.0 MB");
        assert_eq!(source_pill(PodcastKind::Rss).label, "RSS");
        let mut episode = row(1, Some(today_timestamp), PodcastKind::Rss);
        assert_eq!(status_pill(&episode).label, "New");
        episode.position_ms = 10;
        assert_eq!(status_pill(&episode).label, "Resume");
        episode.played_at = Some(1);
        assert_eq!(status_pill(&episode).label, "Played");
    }

    #[test]
    fn filtering_composes_unplayed_show_and_source() {
        let mut rows = vec![
            row(1, Some(10), PodcastKind::Rss),
            row(2, Some(20), PodcastKind::Youtube),
            row(3, Some(30), PodcastKind::Rss),
        ];
        rows[0].played_at = Some(100);
        let filtered = apply_filter(
            &rows,
            &PodcastFilter {
                unplayed_only: true,
                show: Some("Show".into()),
                source: Some(PodcastKind::Youtube),
                downloaded_only: false,
            },
        );
        assert_eq!(filtered.iter().map(|row| row.id).collect::<Vec<_>>(), [2]);
    }

    /// `SRC-10` addendum (Block B2): the "Downloaded" filter matches only
    /// episodes with a file on disk — would go red if `downloaded_only`
    /// were ignored, since one row here has no `downloaded_path` at all.
    #[test]
    fn src_10_downloaded_only_filter_matches_files_on_disk_not_download_state() {
        let mut on_disk = row(1, Some(10), PodcastKind::Rss);
        on_disk.downloaded_path = Some("/music/ep1.mp3".into());
        let not_downloaded = row(2, Some(20), PodcastKind::Rss);
        let rows = vec![on_disk, not_downloaded];

        let filtered = apply_filter(
            &rows,
            &PodcastFilter {
                downloaded_only: true,
                ..PodcastFilter::default()
            },
        );

        assert_eq!(filtered.iter().map(|row| row.id).collect::<Vec<_>>(), [1]);
        assert!(active(&PodcastFilter {
            downloaded_only: true,
            ..PodcastFilter::default()
        }));
    }

    #[test]
    fn default_sort_is_date_descending_with_unknown_dates_last() {
        let mut rows = vec![
            row(1, None, PodcastKind::Rss),
            row(2, Some(10), PodcastKind::Rss),
            row(3, Some(30), PodcastKind::Rss),
        ];
        sort_newest_first(&mut rows);
        assert_eq!(rows.iter().map(|row| row.id).collect::<Vec<_>>(), [3, 2, 1]);
    }

    #[test]
    fn src_5_source_summary_counts_unplayed_downloads_and_latest_episode() {
        let mut first = row(1, Some(10), PodcastKind::Rss);
        first.downloaded_bytes = Some(2_000_000);
        let mut second = row(2, Some(20), PodcastKind::Rss);
        second.downloaded_bytes = Some(3_000_000);
        second.played_at = Some(30);
        let group = SourceGroup {
            subscription_id: 7,
            title: "Show".into(),
            author: Some("Publisher".into()),
            image_url: None,
            kind: PodcastKind::Rss,
            sync_to_phone: true,
            episodes: vec![first, second],
        };

        let states = BTreeMap::from([
            (1, DownloadState::Downloaded { bytes: 2_000_000 }),
            (2, DownloadState::Downloaded { bytes: 3_000_000 }),
        ]);
        assert_eq!(
            source_summary(&group, &states),
            SourceSummary {
                episode_count: 2,
                unplayed_count: 1,
                downloaded_bytes: 5_000_000,
                latest_published_at: Some(20),
            }
        );
    }

    #[test]
    fn pod_9_filtered_children_keep_the_full_source_summary() {
        let mut played = row(1, Some(10), PodcastKind::Rss);
        played.played_at = Some(30);
        let unplayed = row(2, Some(20), PodcastKind::Rss);
        let group = SourceGroup {
            subscription_id: 7,
            title: "Show".into(),
            author: None,
            image_url: None,
            kind: PodcastKind::Rss,
            sync_to_phone: false,
            episodes: vec![played, unplayed],
        };

        let rendered = rendered_source_groups(
            &[group],
            &PodcastFilter {
                unplayed_only: true,
                ..PodcastFilter::default()
            },
            &BTreeMap::new(),
        );

        assert_eq!(rendered[0].group.episodes.len(), 1);
        assert_eq!(rendered[0].summary.episode_count, 2);
        assert_eq!(rendered[0].summary.unplayed_count, 1);
        assert_eq!(rendered[0].summary.latest_published_at, Some(20));
    }
}
