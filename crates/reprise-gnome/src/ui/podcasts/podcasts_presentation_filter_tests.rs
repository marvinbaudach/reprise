use super::*;

fn row(id: i64) -> EpisodeRow {
    EpisodeRow {
        id,
        subscription_id: 7,
        guid: format!("g{id}"),
        title: format!("Episode {id}"),
        show: "Show".into(),
        show_image_url: None,
        image_url: None,
        kind: PodcastKind::Rss,
        audio_url: "https://example.test/episode.mp3".into(),
        page_url: None,
        published_at: Some(id),
        duration_secs: Some(60),
        downloaded_path: None,
        downloaded_bytes: None,
        played_at: None,
        position_ms: 0,
        first_seen_at: id,
        is_new: false,
    }
}

#[test]
fn src_13_only_the_hiding_facet_is_dropped_for_an_episode() {
    let mut episode = row(1);
    episode.played_at = Some(20);
    episode.downloaded_path = Some("/music/episode.mp3".into());
    let filter = PodcastFilter {
        unplayed_only: true,
        source: Some(PodcastKind::Rss),
        downloaded_only: true,
    };

    assert_eq!(
        filter_without_hiding(&episode, &filter),
        PodcastFilter {
            unplayed_only: false,
            source: Some(PodcastKind::Rss),
            downloaded_only: true,
        }
    );
}

#[test]
fn src_13_a_visible_episode_leaves_every_facet_standing() {
    let mut episode = row(1);
    episode.downloaded_path = Some("/music/episode.mp3".into());
    let filter = PodcastFilter {
        unplayed_only: true,
        source: Some(PodcastKind::Rss),
        downloaded_only: true,
    };

    assert_eq!(filter_without_hiding(&episode, &filter), filter);
}

#[test]
fn src_13_a_channel_whose_episodes_all_fail_the_filter_drops_that_facet() {
    let mut first = row(1);
    first.played_at = Some(30);
    let mut second = row(2);
    second.played_at = Some(40);
    let group = SourceGroup {
        subscription_id: 7,
        title: "Show".into(),
        author: None,
        image_url: None,
        kind: PodcastKind::Rss,
        sync_to_phone: false,
        episodes: vec![first, second],
    };
    let filter = PodcastFilter {
        unplayed_only: true,
        source: Some(PodcastKind::Rss),
        downloaded_only: false,
    };

    assert_eq!(
        filter_without_hiding_group(&group, &filter),
        PodcastFilter {
            unplayed_only: false,
            source: Some(PodcastKind::Rss),
            downloaded_only: false,
        }
    );
}
