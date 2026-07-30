//! Pure podcast row formatting, filtering, and sorting.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};
use reprise_core::podcasts::download_state::DownloadState;
use reprise_core::podcasts::{EpisodeRow, EpisodeStatus, PodcastKind, SourceGroup};

use super::podcasts_context_menu::PodcastSyncDevice;
use crate::ui::strings;

/// The filter the podcast view applies, which is exactly the filter the core
/// persists — so it *is* the core's type rather than a field-for-field copy of
/// it. The copy that used to live here had the same four fields and the same
/// derives, and every round trip through the database had to keep the two
/// spellings in step by hand.
///
/// (`SRC-10` addendum, Block B2: `downloaded_only` is the "Downloaded" chip —
/// it matches only episodes with a file on disk right now, not a queued or
/// downloading one.)
pub(super) type PodcastFilter = reprise_core::podcasts::config::PodcastFilterConfig;

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
/// "new" uses the same definition as the per-group facts line
/// (`SourceSummary::unplayed_count`, i.e. `played_at.is_none()`, which
/// includes in-progress "Resume" episodes) so the aggregate and the
/// per-group counts never disagree about what counts as new.
pub(super) fn library_summary(groups: &[SourceGroup]) -> LibrarySummary {
    let mut episodes = 0_usize;
    let mut new = 0_usize;
    for group in groups {
        episodes += group.episodes.len();
        new += group
            .episodes
            .iter()
            .filter(|episode| episode.played_at.is_none())
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
        return String::new();
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

pub(super) fn detail_line<'a>(parts: impl IntoIterator<Item = &'a str>) -> String {
    parts
        .into_iter()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
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

/// `POD-12` / `D3`: whether this channel is selected for at least one
/// currently connected device — the single read-only fact the "On phone"
/// indicator mirrors on both the channel list (`podcasts_groups::
/// group_header`) and the channel detail page (`youtube_channel_detail::
/// build_header`). Pure projection of state that already lives in
/// `podcast_subscription_devices`; nothing about this function's shape lets
/// a caller write the selection back — it takes no database handle and
/// returns a plain `bool`, never a handle to mutate anything.
#[must_use]
pub(super) fn on_phone(
    connected_devices: &[PodcastSyncDevice],
    selected_device_ids: &[String],
) -> bool {
    connected_devices
        .iter()
        .any(|device| selected_device_ids.contains(&device.id))
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
            image_url: None,
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
        assert_eq!(duration(Some(4_533)), "1 h 15");
        assert_eq!(file_size(Some(41_943_040)), Some("40.0 MB".to_owned()));
        assert_eq!(source_pill(PodcastKind::Rss).label, "RSS");
        let mut episode = row(1, Some(today_timestamp), PodcastKind::Rss);
        assert_eq!(status_pill(&episode).label, "New");
        episode.position_ms = 10;
        assert_eq!(status_pill(&episode).label, "Resume");
        episode.played_at = Some(1);
        assert_eq!(status_pill(&episode).label, "Played");
    }

    #[test]
    fn duration_uses_unambiguous_minute_and_hour_boundaries() {
        let cases = [
            (None, ""),
            (Some(-1), ""),
            (Some(0), "< 1 min"),
            (Some(59), "< 1 min"),
            (Some(60), "1 min"),
            (Some(3_599), "59 min"),
            (Some(3_600), "1 h 00"),
            (Some(7_500), "2 h 05"),
        ];

        for (value, expected) in cases {
            assert_eq!(duration(value), expected, "duration {value:?}");
        }
    }

    #[test]
    fn file_size_omits_unknown_zero_and_negative_values() {
        assert_eq!(file_size(None), None);
        assert_eq!(file_size(Some(-1)), None);
        assert_eq!(file_size(Some(0)), None);
        assert_eq!(file_size(Some(1_048_576)), Some("1.0 MB".to_owned()));
        assert_eq!(file_size(Some(1_073_741_824)), Some("1.0 GB".to_owned()));
    }

    #[test]
    fn missing_dates_and_detail_parts_render_no_placeholders_or_empty_separators() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 26).unwrap();

        assert_eq!(relative_date(None, today), "");
        assert_eq!(
            detail_line(["", "", strings::PODCAST_STATUS_NEW]),
            strings::PODCAST_STATUS_NEW
        );
        assert_eq!(
            detail_line(["Today", "", strings::PODCAST_STATUS_NEW]),
            "Today · New"
        );
        assert_eq!(
            strings::podcast_group_facts("15 episodes", 0, "", ""),
            "15 episodes · 0 new"
        );
    }

    #[test]
    fn author_line_hides_title_prefixes_but_keeps_distinct_publishers() {
        assert_eq!(author_line("The Daily", Some("The Daily")), None);
        assert_eq!(
            author_line("The Daily – News Briefing", Some("The Daily")),
            None
        );
        assert_eq!(
            author_line("The Daily", Some("The New York Times")),
            Some("The New York Times")
        );
        assert_eq!(author_line("Artist Notes", Some("Art")), Some("Art"));
        assert_eq!(author_line("Show", Some("   ")), None);
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

    /// `G2` (design 6a): the header line's "new" figure must sum the same
    /// unplayed definition as the per-group facts (`played_at.is_none()`,
    /// so a "Resume" episode still counts as new) across every group, not
    /// just the "New" status — this would go red if the count were narrowed
    /// to `EpisodeStatus::New` only, or if it summed distinct show titles
    /// instead of episodes.
    #[test]
    fn pod_9_library_summary_counts_shows_episodes_and_new_across_all_groups() {
        let mut played = row(1, Some(10), PodcastKind::Rss);
        played.played_at = Some(30);
        let unplayed = row(2, Some(20), PodcastKind::Rss);
        let mut resuming = row(3, Some(15), PodcastKind::Rss);
        resuming.position_ms = 5_000;
        let group_a = SourceGroup {
            subscription_id: 1,
            title: "Show A".into(),
            author: None,
            image_url: None,
            kind: PodcastKind::Rss,
            sync_to_phone: false,
            episodes: vec![played, unplayed],
        };
        let group_b = SourceGroup {
            subscription_id: 2,
            title: "Show B".into(),
            author: None,
            image_url: None,
            kind: PodcastKind::Rss,
            sync_to_phone: false,
            episodes: vec![resuming],
        };

        let summary = library_summary(&[group_a, group_b]);

        assert_eq!(
            summary,
            LibrarySummary {
                shows: 2,
                episodes: 3,
                new: 2,
            }
        );
    }

    /// `G2`: an empty library must not fabricate counts.
    #[test]
    fn pod_9_library_summary_is_zero_for_no_subscriptions() {
        assert_eq!(library_summary(&[]), LibrarySummary::default());
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

    /// `POD-12` / `D3`: the "On phone" indicator must track the selection
    /// exactly — on the moment a connected device is added to the
    /// selection, off the moment it is removed, and unaffected by devices
    /// that are not currently connected.
    #[test]
    fn pod_12_on_phone_reflects_the_toggle() {
        let phone = PodcastSyncDevice {
            id: "mtp:phone".into(),
            name: "Phone".into(),
        };
        let tablet = PodcastSyncDevice {
            id: "mtp:tablet".into(),
            name: "Tablet".into(),
        };

        // Nothing selected yet.
        assert!(!on_phone(std::slice::from_ref(&phone), &[]));

        // Selected, but only for a device that is not currently connected.
        assert!(!on_phone(
            std::slice::from_ref(&phone),
            &["mtp:tablet".to_owned()]
        ));

        // Selected for the connected device: the toggle just turned on.
        assert!(on_phone(
            std::slice::from_ref(&phone),
            &["mtp:phone".to_owned(), "mtp:tablet".to_owned()]
        ));

        // A second connected device also counts.
        assert!(on_phone(
            &[phone.clone(), tablet],
            &["mtp:tablet".to_owned()]
        ));

        // Un-toggled again: back to false.
        assert!(!on_phone(&[phone], &[]));
    }
}
