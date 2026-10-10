//! Small data projections used by the grouped source view.

use reprise_core::db::Db;
use reprise_core::podcasts::{self, PodcastKind, SourceGroup};

pub(super) fn episode_ids_in_rendered_order(groups: &[SourceGroup]) -> Vec<i64> {
    groups
        .iter()
        .flat_map(|group| group.episodes.iter().map(|episode| episode.id))
        .collect()
}

/// The footer label ("Updated 2 minutes ago") for the sources of `kind`, or
/// `None` while none of them was ever fetched successfully.
pub(super) fn last_updated_text(conn: &Db, kind: PodcastKind) -> Option<String> {
    last_updated_text_at(conn, kind, chrono::Utc::now().timestamp())
}

/// The bare age ("2 minutes ago") that sentences such as the cached-content
/// banner embed, never carrying the "Updated" label; `None` while none of the
/// sources of `kind` was ever fetched successfully.
pub(super) fn last_checked_age(conn: &Db, kind: PodcastKind) -> Option<String> {
    last_checked_age_at(conn, kind, chrono::Utc::now().timestamp())
}

fn last_updated_text_at(conn: &Db, kind: PodcastKind, now: i64) -> Option<String> {
    super::podcasts_presentation::updated_ago(last_success_at(conn, kind), now)
}

fn last_checked_age_at(conn: &Db, kind: PodcastKind, now: i64) -> Option<String> {
    super::podcasts_presentation::age_phrase(last_success_at(conn, kind), now)
}

/// A failed attempt (a forced retry included) never moves this: the age says
/// how old the cached episodes are, not when the app last tried.
fn last_success_at(conn: &Db, kind: PodcastKind) -> Option<i64> {
    podcasts::store::last_successful_fetch_at(conn, kind)
        .ok()
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::podcasts::store::{FetchSuccess, NewSubscription};
    use reprise_core::podcasts::{EpisodeRow, SourceGroup};

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

    const NOW: i64 = 1_800_000_000;
    const THREE_DAYS: i64 = 3 * 24 * 60 * 60;

    fn subscribe(db: &Db, kind: PodcastKind, url: &str) -> i64 {
        podcasts::store::add_or_restore(
            db,
            &NewSubscription {
                kind,
                feed_url: url.to_owned(),
                title: url.to_owned(),
                author: None,
                image_url: None,
                auto_download: false,
            },
            1,
        )
        .unwrap()
    }

    fn fetched_ok(db: &Db, id: i64, at: i64) {
        podcasts::store::update_fetch_success(
            db,
            id,
            at,
            FetchSuccess {
                etag: None,
                last_modified: None,
                title: None,
                author: None,
                image_url: None,
            },
        )
        .unwrap();
    }

    #[test]
    fn the_banner_age_is_the_last_success_of_the_views_own_kind() {
        let db = Db::open_in_memory().unwrap();
        let podcast = subscribe(&db, PodcastKind::Rss, "https://example.test/feed");
        let channel = subscribe(&db, PodcastKind::Youtube, "https://example.test/channel");
        fetched_ok(&db, podcast, NOW - THREE_DAYS);
        // A YouTube fetch that just succeeded says nothing about the podcasts.
        fetched_ok(&db, channel, NOW);

        assert_eq!(
            last_checked_age_at(&db, PodcastKind::Rss, NOW).as_deref(),
            Some("3 days ago")
        );
        assert_eq!(
            last_updated_text_at(&db, PodcastKind::Rss, NOW).as_deref(),
            Some("Updated 3 days ago")
        );
        assert_eq!(
            last_checked_age_at(&db, PodcastKind::Youtube, NOW).as_deref(),
            Some("just now")
        );
    }

    #[test]
    fn a_kind_that_never_fetched_successfully_has_no_age() {
        let db = Db::open_in_memory().unwrap();
        subscribe(&db, PodcastKind::Rss, "https://example.test/feed");
        let channel = subscribe(&db, PodcastKind::Youtube, "https://example.test/channel");
        fetched_ok(&db, channel, NOW);

        assert_eq!(last_checked_age_at(&db, PodcastKind::Rss, NOW), None);
        assert_eq!(last_updated_text_at(&db, PodcastKind::Rss, NOW), None);
    }
}
