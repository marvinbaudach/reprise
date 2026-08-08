//! Projection of provider search output into add-dialog result candidates.
//!
//! Keeping this beside the dialog rather than inside it separates *what a
//! result says* from *how the dialog behaves*, and keeps both files inside the
//! file-size gate.

use chrono::{DateTime, Local, Utc};
use gtk4::prelude::*;
use reprise_core::podcasts::discovery::Candidate;
use reprise_core::podcasts::{self, PodcastKind};

use crate::ui::strings;

/// `SRC-9`: the subscriber count is optional context, so it is appended only
/// when the channel actually publishes one.
pub(super) fn youtube_subtitle(matching_videos: usize, followers: Option<u64>) -> String {
    let matches = strings::podcast_youtube_channel_matches(matching_videos);
    match followers {
        Some(followers) => format!(
            "{matches} · {}",
            strings::podcast_subscriber_count(followers)
        ),
        None => matches,
    }
}

pub(super) fn last_episode_segment(last_episode: Option<i64>, now: i64) -> Option<String> {
    let last_episode = last_episode?;
    let days = usize::try_from((now - last_episode).max(0) / 86_400).unwrap_or(usize::MAX);
    match days {
        0 => Some(strings::text(strings::PODCAST_LAST_EPISODE_TODAY)),
        1 => Some(strings::text(strings::PODCAST_LAST_EPISODE_YESTERDAY)),
        2..=6 => Some(strings::podcast_last_episode_days(days)),
        7..=34 => Some(strings::podcast_last_episode_weeks(days / 7)),
        35..=364 => Some(strings::podcast_last_episode_months(days / 30)),
        _ => {
            let date = DateTime::<Utc>::from_timestamp(last_episode, 0)?;
            Some(strings::podcast_last_episode_on(
                &date.with_timezone(&Local).format("%b %Y").to_string(),
            ))
        }
    }
}

pub(super) fn rss_subtitle(author: Option<&str>, last_episode: Option<i64>, now: i64) -> String {
    author
        .filter(|author| !author.is_empty())
        .map(str::to_owned)
        .into_iter()
        .chain(last_episode_segment(last_episode, now))
        .collect::<Vec<_>>()
        .join(" · ")
}

pub(super) fn rss_candidate(row: podcasts::itunes::SearchResult) -> Candidate {
    let subtitle = rss_subtitle(
        row.author.as_deref(),
        row.last_episode,
        Utc::now().timestamp(),
    );
    Candidate {
        kind: PodcastKind::Rss,
        title: row.title,
        subtitle,
        author: row.author,
        image_url: row.image_url,
        url: row.feed_url,
        identity_guids: Vec::new(),
    }
}

pub(super) fn youtube_candidate(row: podcasts::ytdlp::YtDlpChannel) -> Candidate {
    Candidate {
        kind: PodcastKind::Youtube,
        title: row.title,
        subtitle: youtube_subtitle(row.matching_video_count, row.follower_count),
        author: None,
        image_url: row.image_url,
        url: row.url,
        identity_guids: row.matching_video_ids,
    }
}

pub(super) fn result_section() -> gtk4::Box {
    let section = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    section.add_css_class("reprise-podcast-result-section");
    section
}

pub(super) fn clear(parent: &gtk4::Box) {
    while let Some(child) = parent.first_child() {
        parent.remove(&child);
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    #[test]
    fn src_18_the_freshness_scale_walks_its_boundaries() {
        let now = Utc
            .with_ymd_and_hms(2026, 8, 15, 12, 0, 0)
            .unwrap()
            .timestamp();
        let cases = [
            (-2, "New today"),
            (0, "New today"),
            (1, "New yesterday"),
            (2, "New 2 days ago"),
            (6, "New 6 days ago"),
            (7, "New 1 week ago"),
            (13, "New 1 week ago"),
            (14, "New 2 weeks ago"),
            (34, "New 4 weeks ago"),
            (35, "1 month ago"),
            (364, "12 months ago"),
            (365, "Last Aug 2025"),
        ];

        for (days, expected) in cases {
            let published_at = now - i64::from(days) * 86_400;
            assert_eq!(
                last_episode_segment(Some(published_at), now).as_deref(),
                Some(expected),
                "age {days} days"
            );
        }
    }

    #[test]
    fn src_18_a_row_without_a_date_or_an_author_leaves_no_separator_behind() {
        let now = Utc
            .with_ymd_and_hms(2026, 8, 15, 12, 0, 0)
            .unwrap()
            .timestamp();
        let four_days_ago = now - 4 * 86_400;

        assert_eq!(
            rss_subtitle(Some("Ada"), Some(four_days_ago), now),
            "Ada · New 4 days ago"
        );
        assert_eq!(rss_subtitle(Some("Ada"), None, now), "Ada");
        assert_eq!(
            rss_subtitle(None, Some(four_days_ago), now),
            "New 4 days ago"
        );
        assert_eq!(rss_subtitle(None, None, now), "");
    }

    #[test]
    fn src_9_channel_rows_show_a_subscriber_count_only_when_there_is_one() {
        let with_count = youtube_subtitle(3, Some(62_400));
        let without = youtube_subtitle(3, None);

        assert!(with_count.contains("62.4k"), "{with_count}");
        assert!(
            with_count.starts_with(&without),
            "the count is appended context, not a replacement"
        );
        assert!(
            !without.contains("subscriber"),
            "a hidden count is omitted, never rendered as zero or unknown"
        );
    }

    #[test]
    fn src_9_subscriber_counts_are_compact_and_keep_their_magnitude() {
        assert_eq!(strings::podcast_subscriber_count(487), "487 subscribers");
        assert_eq!(
            strings::podcast_subscriber_count(62_400),
            "62.4k subscribers"
        );
        assert_eq!(strings::podcast_subscriber_count(62_000), "62k subscribers");
        assert_eq!(
            strings::podcast_subscriber_count(1_200_000),
            "1.2M subscribers"
        );
    }
}
