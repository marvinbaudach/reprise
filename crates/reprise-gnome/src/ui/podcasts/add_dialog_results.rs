//! Projection of provider search output into add-dialog result candidates.
//!
//! Keeping this beside the dialog rather than inside it separates *what a
//! result says* from *how the dialog behaves*, and keeps both files inside the
//! file-size gate.

use chrono::{DateTime, Datelike, Local, Utc};
use gtk4::prelude::*;
use reprise_core::podcasts::discovery::Candidate;
use reprise_core::podcasts::{self, PodcastKind};

use crate::ui::search_highlight::{self, HighlightPalette};
use crate::ui::strings;

const SECONDS_PER_DAY: i64 = 86_400;
const LAST_EPISODE_ABSOLUTE_AFTER_DAYS: usize = 365;

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

/// `SRC-23`: published counts sort descending; ties and every missing count
/// retain provider relevance order. A real published zero therefore remains
/// ahead of a hidden count instead of becoming indistinguishable from it.
pub(super) fn subscriber_order(counts: &[Option<u64>]) -> Vec<usize> {
    let mut order = (0..counts.len()).collect::<Vec<_>>();
    order.sort_by(|left, right| match (counts[*left], counts[*right]) {
        (Some(left), Some(right)) => right.cmp(&left),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    order
}

pub(super) fn last_episode_segment(last_episode: Option<i64>, now: i64) -> Option<String> {
    let last_episode = last_episode?;
    let days = last_episode_age_days(last_episode, now);
    if days >= LAST_EPISODE_ABSOLUTE_AFTER_DAYS {
        let date = DateTime::<Utc>::from_timestamp(last_episode, 0)?;
        let date = date.with_timezone(&Local).date_naive();
        let date = crate::ui::date_format::current().date.render(
            Some(date.year()),
            Some(date.month()),
            None,
        );
        return Some(strings::podcast_last_episode_on(&date));
    }
    match days {
        0 => Some(strings::text(strings::PODCAST_LAST_EPISODE_TODAY)),
        1 => Some(strings::text(strings::PODCAST_LAST_EPISODE_YESTERDAY)),
        2..=6 => Some(strings::podcast_last_episode_days(days)),
        7..=34 => Some(strings::podcast_last_episode_weeks(days / 7)),
        35..=64 => Some(strings::podcast_last_episode_months(1)),
        _ => Some(strings::podcast_last_episode_months(days / 30)),
    }
}

fn last_episode_age_days(last_episode: i64, now: i64) -> usize {
    usize::try_from((now - last_episode).max(0) / SECONDS_PER_DAY).unwrap_or(usize::MAX)
}

/// `SRC-20`: keep Apple's relevance order inside each side of the freshness
/// boundary. A missing date is evidence of nothing and therefore stays with
/// the fresh results.
pub(super) fn partition_dormant_search_results(
    rows: &mut [podcasts::itunes::SearchResult],
    now: i64,
) {
    rows.sort_by_key(|row| {
        row.last_episode.is_some_and(|last_episode| {
            last_episode_age_days(last_episode, now) >= LAST_EPISODE_ABSOLUTE_AFTER_DAYS
        })
    });
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct SearchResultMarkup {
    pub(super) title: String,
    pub(super) subtitle: String,
    pub(super) unexplained_match: bool,
}

/// `SRC-21`: highlight only provider fields that can explain a search match.
/// The generated freshness suffix is escaped separately and never becomes a
/// target even when it happens to contain the query.
pub(super) fn search_result_markup(
    kind: PodcastKind,
    title: &str,
    subtitle: &str,
    author: Option<&str>,
    query: Option<&str>,
    palette: Option<&HighlightPalette>,
) -> SearchResultMarkup {
    let (title, title_matches) = highlighted_or_escaped(title, query, palette);
    let (subtitle, author_matches) = author
        .filter(|author| !author.is_empty())
        .and_then(|author| subtitle.strip_prefix(author).map(|suffix| (author, suffix)))
        .map_or_else(
            || (gtk4::glib::markup_escape_text(subtitle).to_string(), false),
            |(author, suffix)| {
                let (author, author_matches) = highlighted_or_escaped(author, query, palette);
                let subtitle = format!("{}{}", author, gtk4::glib::markup_escape_text(suffix));
                (subtitle, author_matches)
            },
        );
    let has_query =
        query.is_some_and(|query| search_highlight::highlight_markup(query, query, None).is_some());
    SearchResultMarkup {
        title,
        subtitle,
        unexplained_match: kind == PodcastKind::Rss
            && has_query
            && !title_matches
            && !author_matches,
    }
}

fn highlighted_or_escaped(
    text: &str,
    query: Option<&str>,
    palette: Option<&HighlightPalette>,
) -> (String, bool) {
    let highlighted =
        query.and_then(|query| search_highlight::highlight_markup(text, query, palette));
    let matches = highlighted.is_some();
    (
        highlighted.unwrap_or_else(|| gtk4::glib::markup_escape_text(text).to_string()),
        matches,
    )
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
        follower_count: None,
        channel_id: None,
        matching_video_count: None,
    }
}

pub(super) fn youtube_candidate(row: podcasts::ytdlp::YtDlpChannel) -> Candidate {
    let follower_count = row.follower_count;
    Candidate {
        kind: PodcastKind::Youtube,
        title: row.title,
        subtitle: youtube_subtitle(row.matching_video_count, follower_count),
        author: None,
        image_url: row.image_url,
        url: row.url,
        identity_guids: row.matching_video_ids,
        follower_count,
        channel_id: Some(row.id),
        matching_video_count: Some(row.matching_video_count),
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

    fn palette() -> HighlightPalette {
        HighlightPalette::new("#2ec8a6", "#2ec8a6")
    }

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
            (7, "New last week"),
            (13, "New last week"),
            (14, "New 2 weeks ago"),
            (34, "New 4 weeks ago"),
            (35, "Last month"),
            (64, "Last month"),
            (65, "2 months ago"),
            (364, "12 months ago"),
        ];

        for (days, expected) in cases {
            let published_at = now - i64::from(days) * 86_400;
            assert_eq!(
                last_episode_segment(Some(published_at), now).as_deref(),
                Some(expected),
                "age {days} days"
            );
        }
        let published_at = now - 365 * 86_400;
        let date = DateTime::<Utc>::from_timestamp(published_at, 0)
            .unwrap()
            .with_timezone(&Local)
            .date_naive();
        let date = crate::ui::date_format::current().date.render(
            Some(date.year()),
            Some(date.month()),
            None,
        );
        assert_eq!(
            last_episode_segment(Some(published_at), now),
            Some(strings::podcast_last_episode_on(&date))
        );
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
    fn src_20_search_keeps_fresh_then_dormant_apple_relevance_order() {
        let now = Utc
            .with_ymd_and_hms(2026, 8, 15, 12, 0, 0)
            .unwrap()
            .timestamp();
        let mut rows = podcasts::itunes::parse_results(
            r#"{"results":[
              {"collectionName":"Dormant A","feedUrl":"https://e.test/dormant-a","releaseDate":"2025-08-15T12:00:00Z"},
              {"collectionName":"Fresh A","feedUrl":"https://e.test/fresh-a","releaseDate":"2026-08-14T12:00:00Z"},
              {"collectionName":"Undated","feedUrl":"https://e.test/undated"},
              {"collectionName":"Fresh B","feedUrl":"https://e.test/fresh-b","releaseDate":"2025-08-16T12:00:00Z"},
              {"collectionName":"Dormant B","feedUrl":"https://e.test/dormant-b","releaseDate":"2020-01-01T00:00:00Z"}
            ]}"#,
        )
        .unwrap();

        partition_dormant_search_results(&mut rows, now);

        assert_eq!(
            rows.iter()
                .map(|row| row.title.as_str())
                .collect::<Vec<_>>(),
            ["Fresh A", "Undated", "Fresh B", "Dormant A", "Dormant B"],
            "the partition keeps Apple's order within both groups and treats no date as fresh"
        );
    }

    #[test]
    fn src_21_search_accents_title_and_author_but_never_freshness() {
        let freshness_only_match = search_result_markup(
            PodcastKind::Rss,
            "Weekly <News>",
            "Sean & Sons · New 1 week ago",
            Some("Sean & Sons"),
            Some("week"),
            Some(&palette()),
        );
        assert_eq!(
            freshness_only_match.title,
            "<span foreground=\"#2ec8a6\" background=\"#2ec8a6\" bgalpha=\"18%\" weight=\"bold\">Week</span>ly &lt;News&gt;"
        );
        assert_eq!(
            freshness_only_match.subtitle, "Sean &amp; Sons · New 1 week ago",
            "a query found only in freshness must never mark that segment"
        );

        let author_match = search_result_markup(
            PodcastKind::Rss,
            "Weekly <News>",
            "Sean & Sons · New 1 week ago",
            Some("Sean & Sons"),
            Some("sean"),
            Some(&palette()),
        );
        assert_eq!(author_match.title, "Weekly &lt;News&gt;");
        assert_eq!(
            author_match.subtitle,
            "<span foreground=\"#2ec8a6\" background=\"#2ec8a6\" bgalpha=\"18%\" weight=\"bold\">Sean</span> &amp; Sons · New 1 week ago"
        );

        let chart = search_result_markup(
            PodcastKind::Rss,
            "Weekly <News>",
            "Sean & Sons · New 1 week ago",
            Some("Sean & Sons"),
            None,
            Some(&palette()),
        );
        assert_eq!(chart.title, "Weekly &lt;News&gt;");
        assert_eq!(chart.subtitle, "Sean &amp; Sons · New 1 week ago");
    }

    #[test]
    fn src_22_only_search_hits_without_a_visible_match_need_an_explanation() {
        let unexplained = search_result_markup(
            PodcastKind::Rss,
            "The Jasta Show",
            "GaS Digital Network · New last week",
            Some("GaS Digital Network"),
            Some("Metalcore"),
            Some(&palette()),
        );
        assert!(unexplained.unexplained_match);

        for (title, author, query) in [
            ("MetalCore & More", Some("Publisher"), Some("metalcore")),
            ("A Show", Some("MetalCore Network"), Some("metalcore")),
            ("The Jasta Show", Some("GaS Digital Network"), None),
        ] {
            let explained = search_result_markup(
                PodcastKind::Rss,
                title,
                author.unwrap_or_default(),
                author,
                query,
                Some(&palette()),
            );
            assert!(
                !explained.unexplained_match,
                "title={title:?}, author={author:?}, query={query:?}"
            );
        }

        let youtube = search_result_markup(
            PodcastKind::Youtube,
            "A Channel",
            "4 matching videos",
            None,
            Some("Metalcore"),
            Some(&palette()),
        );
        assert!(
            !youtube.unexplained_match,
            "an Apple-specific explanation must not leak into YouTube search"
        );
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
    fn src_9_youtube_candidates_keep_the_stable_channel_join_key_and_count() {
        let candidate = youtube_candidate(podcasts::ytdlp::YtDlpChannel {
            id: "UC-stable".into(),
            title: "Visible".into(),
            url: "https://www.youtube.com/channel/UC-stable".into(),
            image_url: None,
            matching_video_count: 3,
            matching_video_ids: vec!["video-1".into()],
            follower_count: Some(62_400),
        });

        assert_eq!(candidate.channel_id.as_deref(), Some("UC-stable"));
        assert_eq!(candidate.follower_count, Some(62_400));
        assert!(candidate.subtitle.contains("62.4k"));
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

    #[test]
    fn src_23_largest_first_keeps_ties_and_missing_counts_in_relevance_order() {
        let counts = [None, Some(50), Some(100), Some(50), None, Some(0)];

        assert_eq!(subscriber_order(&counts), [2, 1, 3, 5, 0, 4]);
    }
}
