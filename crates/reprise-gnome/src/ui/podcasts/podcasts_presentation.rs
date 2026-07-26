//! Pure podcast row formatting, filtering, and sorting.

use std::cmp::Ordering;

use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};
use reprise_core::podcasts::{EpisodeRow, EpisodeStatus, PodcastKind};

use crate::ui::strings;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct PodcastFilter {
    pub unplayed_only: bool,
    pub show: Option<String>,
    pub source: Option<PodcastKind>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Pill {
    pub label: &'static str,
    pub icon: Option<&'static str>,
    pub css_class: &'static str,
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
}

pub(super) fn apply_filter(rows: &[EpisodeRow], filter: &PodcastFilter) -> Vec<EpisodeRow> {
    rows.iter()
        .filter(|row| matches_filter(row, filter))
        .cloned()
        .collect()
}

pub(super) fn active(filter: &PodcastFilter) -> bool {
    filter.unplayed_only || filter.show.is_some() || filter.source.is_some()
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
            played_at: None,
            position_ms: 0,
            first_seen_at: id,
        }
    }

    #[test]
    fn pod_1_presentation_formats_date_length_source_and_status() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 26).unwrap();
        let today_timestamp = today.and_hms_opt(12, 0, 0).unwrap().and_utc().timestamp();
        assert_eq!(relative_date(Some(today_timestamp), today), "Today");
        assert_eq!(
            relative_date(Some(today_timestamp - 86_400), today),
            "Yesterday"
        );
        assert_eq!(duration(Some(4_533)), "1:15");
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
            },
        );
        assert_eq!(filtered.iter().map(|row| row.id).collect::<Vec<_>>(), [2]);
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
}
