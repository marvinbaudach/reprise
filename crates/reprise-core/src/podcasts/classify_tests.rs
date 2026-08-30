//! Tests for `podcasts::classify` — `AC-26`'s one classifying request.

use std::cell::RefCell;
use std::path::Path;

use crate::db::Db;
use crate::podcasts::feed::ParsedEpisode;
use crate::podcasts::pipeline::YoutubeFetcher;
use crate::podcasts::store::{add_or_restore, episode, upsert_episode, NewSubscription};
use crate::podcasts::{PodcastError, PodcastKind};

use super::{classify_youtube_episode, needs_classification, EpisodeClassification};

/// Records every URL it was asked about, so a test can prove the request was
/// spent once — or not at all.
#[derive(Default)]
struct RecordingFetcher {
    answer: EpisodeClassification,
    asked: RefCell<Vec<String>>,
}

impl RecordingFetcher {
    fn answering(category: Option<&str>, duration_secs: Option<i64>) -> Self {
        Self {
            answer: EpisodeClassification {
                media_category: category.map(str::to_owned),
                duration_secs,
            },
            asked: RefCell::new(Vec::new()),
        }
    }
}

impl YoutubeFetcher for RecordingFetcher {
    fn list(
        &self,
        _url: &str,
        _limit: usize,
    ) -> Result<crate::podcasts::feed::ParsedFeed, PodcastError> {
        unreachable!("classification never lists a channel")
    }

    fn download(&self, _url: &str, _destination: &Path) -> Result<(), PodcastError> {
        unreachable!("classification never downloads")
    }

    fn classify(&self, url: &str) -> Result<EpisodeClassification, PodcastError> {
        self.asked.borrow_mut().push(url.to_owned());
        Ok(self.answer.clone())
    }
}

fn youtube_episode(db: &Db) -> i64 {
    let subscription_id = add_or_restore(
        db,
        &NewSubscription {
            kind: PodcastKind::Youtube,
            feed_url: "https://youtube.test/channel".to_owned(),
            title: "Channel".to_owned(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        10,
    )
    .unwrap();
    upsert_episode(
        db,
        subscription_id,
        &ParsedEpisode {
            guid: "video-1".to_owned(),
            title: "Video".to_owned(),
            image_url: None,
            audio_url: "https://youtube.test/watch?v=1".to_owned(),
            page_url: None,
            published_at: Some(20),
            duration_secs: None,
        },
        20,
    )
    .unwrap()
    .unwrap()
    .episode_id
}

fn set_category(db: &Db, episode_id: i64, category: Option<&str>) {
    db.conn()
        .execute(
            "UPDATE podcast_episodes SET media_category = ?2 WHERE id = ?1",
            rusqlite::params![episode_id, category],
        )
        .unwrap();
}

#[test]
fn ac_26_an_unclassified_youtube_episode_is_classified_and_stored() {
    let db = Db::open_in_memory().unwrap();
    let episode_id = youtube_episode(&db);
    let fetcher = RecordingFetcher::answering(Some("Music"), Some(93));

    let learned = classify_youtube_episode(&db, &fetcher, episode_id).unwrap();

    assert_eq!(learned.as_deref(), Some("Music"));
    assert_eq!(
        fetcher.asked.borrow().as_slice(),
        ["https://youtube.test/watch?v=1"]
    );
    let stored = episode(&db, episode_id).unwrap().unwrap();
    assert_eq!(stored.media_category.as_deref(), Some("Music"));
    assert_eq!(
        stored.duration_secs,
        Some(93),
        "the duration rides along on a call that carries it anyway"
    );
}

#[test]
fn ac_26_a_classified_episode_costs_no_request() {
    let db = Db::open_in_memory().unwrap();
    let episode_id = youtube_episode(&db);
    set_category(&db, episode_id, Some("Education"));
    let fetcher = RecordingFetcher::answering(Some("Music"), Some(93));

    assert_eq!(classify_youtube_episode(&db, &fetcher, episode_id).unwrap(), None);

    assert!(
        fetcher.asked.borrow().is_empty(),
        "a stored category is the answer; asking again would be a request for nothing"
    );
    let stored = episode(&db, episode_id).unwrap().unwrap();
    assert_eq!(
        stored.media_category.as_deref(),
        Some("Education"),
        "a stored category is never overwritten by a later classification"
    );
}

#[test]
fn ac_26_a_blank_stored_category_still_counts_as_unclassified() {
    let db = Db::open_in_memory().unwrap();
    let episode_id = youtube_episode(&db);
    set_category(&db, episode_id, Some("   "));
    let fetcher = RecordingFetcher::answering(Some("Music"), None);

    let learned = classify_youtube_episode(&db, &fetcher, episode_id).unwrap();

    assert_eq!(learned.as_deref(), Some("Music"));
}

#[test]
fn ac_26_an_extraction_that_knows_no_category_stores_nothing() {
    let db = Db::open_in_memory().unwrap();
    let episode_id = youtube_episode(&db);
    let fetcher = RecordingFetcher::answering(None, Some(93));

    assert_eq!(classify_youtube_episode(&db, &fetcher, episode_id).unwrap(), None);

    let stored = episode(&db, episode_id).unwrap().unwrap();
    assert_eq!(stored.media_category, None);
    assert_eq!(
        stored.duration_secs, None,
        "an answer with no category writes nothing at all"
    );
}

#[test]
fn ac_26_an_rss_episode_is_never_classified() {
    let db = Db::open_in_memory().unwrap();
    let subscription_id = add_or_restore(
        &db,
        &NewSubscription {
            kind: PodcastKind::Rss,
            feed_url: "https://example.test/feed.xml".to_owned(),
            title: "Show".to_owned(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        10,
    )
    .unwrap();
    let episode_id = upsert_episode(
        &db,
        subscription_id,
        &ParsedEpisode {
            guid: "episode-1".to_owned(),
            title: "Episode".to_owned(),
            image_url: None,
            audio_url: "https://example.test/episode.mp3".to_owned(),
            page_url: None,
            published_at: Some(20),
            duration_secs: None,
        },
        20,
    )
    .unwrap()
    .unwrap()
    .episode_id;
    let fetcher = RecordingFetcher::answering(Some("Music"), Some(93));

    assert_eq!(classify_youtube_episode(&db, &fetcher, episode_id).unwrap(), None);
    assert!(fetcher.asked.borrow().is_empty());
}

#[test]
fn needs_classification_reads_kind_and_category() {
    let db = Db::open_in_memory().unwrap();
    let episode_id = youtube_episode(&db);
    let unclassified = episode(&db, episode_id).unwrap().unwrap();
    assert!(needs_classification(&unclassified));

    set_category(&db, episode_id, Some("Music"));
    let classified = episode(&db, episode_id).unwrap().unwrap();
    assert!(!needs_classification(&classified));
}
