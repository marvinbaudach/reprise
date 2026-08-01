//! The episode order a Shift-click ranges over.

use std::collections::BTreeSet;

use reprise_core::podcasts::{EpisodeRow, PodcastKind, SourceGroup};

use super::rendered_episode_ids;

fn episode(id: i64) -> EpisodeRow {
    EpisodeRow {
        id,
        subscription_id: 1,
        guid: format!("episode-{id}"),
        title: format!("Episode {id}"),
        show: "Show".into(),
        show_image_url: None,
        image_url: None,
        kind: PodcastKind::Rss,
        audio_url: format!("https://example.test/{id}.mp3"),
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

fn group(subscription_id: i64, episodes: Vec<EpisodeRow>) -> SourceGroup {
    SourceGroup {
        subscription_id,
        title: format!("Show {subscription_id}"),
        author: None,
        image_url: None,
        kind: PodcastKind::Rss,
        sync_to_phone: false,
        episodes,
    }
}

#[test]
fn src_14_a_collapsed_group_contributes_no_rows() {
    let groups = vec![
        group(1, vec![episode(10), episode(11)]),
        group(2, vec![episode(20)]),
    ];

    let order = rendered_episode_ids(&groups, &BTreeSet::from([2]), &BTreeSet::new());

    assert_eq!(
        order,
        vec![20],
        "a Shift-click must not reach through a closed expander"
    );
}

#[test]
fn src_14_the_order_runs_across_groups_in_render_order() {
    let groups = vec![
        group(1, vec![episode(10), episode(11)]),
        group(2, vec![episode(20)]),
    ];

    let order = rendered_episode_ids(&groups, &BTreeSet::from([1, 2]), &BTreeSet::new());

    assert_eq!(order, vec![10, 11, 20]);
}

#[test]
fn src_14_a_windowed_group_contributes_only_its_visible_ten() {
    let episodes = (0..12)
        .map(|index| episode(100 + index))
        .collect::<Vec<_>>();
    let groups = vec![group(1, episodes)];
    let expanded = BTreeSet::from([1]);

    let windowed = rendered_episode_ids(&groups, &expanded, &BTreeSet::new());
    assert_eq!(
        windowed.len(),
        10,
        "the preview window caps the group at ten"
    );
    assert_eq!(windowed.first(), Some(&100));
    assert_eq!(windowed.last(), Some(&109));

    let all = rendered_episode_ids(&groups, &expanded, &BTreeSet::from([1]));
    assert_eq!(all.len(), 12, "'Show all' puts every episode in range");
}
