//! Revealing the loaded episode in the grouped list.
//!
//! `SRC-12`'s "how" for the podcast/YouTube surface. *Whether* to reveal is
//! decided in `crate::ui::source_reveal`; this module answers where the
//! episode is and what has to open before it exists as a widget at all.

use reprise_core::podcasts::SourceGroup;

use super::podcasts_episode_window::visible_count;

/// What has to change about the list's expansion state before the loaded
/// episode is a rendered row that can be scrolled to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RevealTarget {
    /// The group that has to be expanded.
    pub(super) subscription_id: i64,
    /// Whether the group's ten-episode preview window also has to be opened —
    /// true when the episode sits past the preview, where no row is built.
    pub(super) needs_full_window: bool,
}

/// Locates `episode_id` in the rendered groups and reports what must open for
/// it to become visible. `None` when the episode is not in this list at all —
/// a filtered-out episode, or the other kind's view.
pub(super) fn reveal_target(
    groups: &[SourceGroup],
    episode_id: i64,
    window_already_expanded: bool,
) -> Option<RevealTarget> {
    let group = groups
        .iter()
        .find(|group| group.episodes.iter().any(|row| row.id == episode_id))?;
    let index = group.episodes.iter().position(|row| row.id == episode_id)?;
    let rendered = visible_count(group.episodes.len(), window_already_expanded);
    Some(RevealTarget {
        subscription_id: group.subscription_id,
        needs_full_window: index >= rendered,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::podcasts::{EpisodeRow, PodcastKind, SourceGroup};

    fn episode(id: i64, subscription_id: i64) -> EpisodeRow {
        EpisodeRow {
            id,
            subscription_id,
            guid: format!("episode-{id}"),
            title: format!("Episode {id}"),
            show: "Show".into(),
            show_image_url: None,
            image_url: None,
            kind: PodcastKind::Youtube,
            audio_url: "https://example.test/e.mp3".into(),
            page_url: None,
            published_at: None,
            duration_secs: None,
            downloaded_path: None,
            downloaded_bytes: None,
            played_at: None,
            position_ms: 0,
            first_seen_at: 1,
            is_new: false,
        }
    }

    fn group(subscription_id: i64, episode_ids: &[i64]) -> SourceGroup {
        SourceGroup {
            subscription_id,
            title: format!("Channel {subscription_id}"),
            author: None,
            image_url: None,
            kind: PodcastKind::Youtube,
            sync_to_phone: false,
            episodes: episode_ids
                .iter()
                .map(|id| episode(*id, subscription_id))
                .collect(),
        }
    }

    #[test]
    fn an_episode_inside_the_preview_window_only_needs_its_group_expanded() {
        let groups = [group(7, &[1, 2, 3])];

        assert_eq!(
            reveal_target(&groups, 2, false),
            Some(RevealTarget {
                subscription_id: 7,
                needs_full_window: false,
            })
        );
    }

    #[test]
    fn an_episode_beyond_the_preview_window_needs_the_window_opened() {
        let ids = (1..=15).collect::<Vec<_>>();
        let groups = [group(7, &ids)];

        // Index 12 (episode 13) sits past the ten-episode preview.
        assert_eq!(
            reveal_target(&groups, 13, false),
            Some(RevealTarget {
                subscription_id: 7,
                needs_full_window: true,
            })
        );
    }

    #[test]
    fn an_already_expanded_window_never_asks_to_be_expanded_again() {
        let ids = (1..=15).collect::<Vec<_>>();
        let groups = [group(7, &ids)];

        assert_eq!(
            reveal_target(&groups, 13, true),
            Some(RevealTarget {
                subscription_id: 7,
                needs_full_window: false,
            })
        );
    }

    #[test]
    fn the_right_group_is_picked_out_of_several() {
        let groups = [group(7, &[1, 2]), group(8, &[3, 4])];

        assert_eq!(
            reveal_target(&groups, 4, false).map(|target| target.subscription_id),
            Some(8)
        );
    }

    #[test]
    fn an_episode_that_is_not_listed_has_nothing_to_reveal() {
        let groups = [group(7, &[1, 2])];

        assert_eq!(reveal_target(&groups, 99, false), None);
        assert_eq!(reveal_target(&[], 1, false), None);
    }
}
