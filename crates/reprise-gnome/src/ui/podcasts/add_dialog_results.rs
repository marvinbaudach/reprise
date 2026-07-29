//! Projection of provider search output into add-dialog result candidates.
//!
//! Keeping this beside the dialog rather than inside it separates *what a
//! result says* from *how the dialog behaves*, and keeps both files inside the
//! file-size gate.

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

pub(super) fn rss_candidate(row: podcasts::itunes::SearchResult) -> Candidate {
    Candidate {
        kind: PodcastKind::Rss,
        title: row.title,
        subtitle: row.author.clone().unwrap_or_default(),
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
    use super::*;

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
