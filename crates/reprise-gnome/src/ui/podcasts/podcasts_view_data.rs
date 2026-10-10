//! Small data projections used by the grouped source view.

use reprise_core::db::Db;
use reprise_core::podcasts::{self, SourceGroup};

pub(super) fn episode_ids_in_rendered_order(groups: &[SourceGroup]) -> Vec<i64> {
    groups
        .iter()
        .flat_map(|group| group.episodes.iter().map(|episode| episode.id))
        .collect()
}

fn last_fetch_at(conn: &Db) -> Option<i64> {
    podcasts::store::active_subscriptions(conn)
        .ok()
        .and_then(|rows| rows.into_iter().filter_map(|row| row.last_fetch_at).max())
}

/// The footer label: "Updated 2 minutes ago".
pub(super) fn last_updated_text(conn: &Db) -> String {
    super::podcasts_presentation::updated_ago(last_fetch_at(conn), chrono::Utc::now().timestamp())
}

/// The bare age ("2 minutes ago") that sentences such as the cached-content
/// banner embed; never carries the "Updated" label.
pub(super) fn last_checked_age(conn: &Db) -> String {
    super::podcasts_presentation::age_phrase(last_fetch_at(conn), chrono::Utc::now().timestamp())
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
            show: format!("Show {subscription_id}"),
            show_image_url: None,
            image_url: None,
            kind: PodcastKind::Rss,
            audio_url: format!("https://example.test/{id}.mp3"),
            page_url: None,
            published_at: Some(id),
            duration_secs: None,
            downloaded_path: None,
            downloaded_bytes: None,
            played_at: (id == 2).then_some(10),
            position_ms: 0,
            first_seen_at: id,
            is_new: false,
            media_category: None,
        }
    }

    fn group(subscription_id: i64, ids: &[i64]) -> SourceGroup {
        SourceGroup {
            subscription_id,
            title: format!("Show {subscription_id}"),
            author: None,
            image_url: None,
            kind: PodcastKind::Rss,
            episodes: ids.iter().map(|id| episode(*id, subscription_id)).collect(),
        }
    }

    #[test]
    fn pod_21_neighbour_snapshot_flattens_every_group_and_collapsed_episode() {
        let groups = vec![group(1, &[3, 2]), group(2, &[9, 8, 7])];

        assert_eq!(episode_ids_in_rendered_order(&groups), vec![3, 2, 9, 8, 7]);
    }
}
