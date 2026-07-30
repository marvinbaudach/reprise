//! `NET-3c`: the runner that notices connectivity returned and replays
//! whatever download work is still pending, in order (F2, design 7f / turn
//! 6's 6e).
//!
//! `wanted_on_device` (`MTP-40`) plus each episode's own download state
//! already record what is pending — there is deliberately no second
//! "offline queue" table that could drift out of sync with them. An episode
//! counts as pending here exactly when it is `wanted_on_device` and still
//! has no local file (`downloaded_path IS NULL`); `device_sync::preparation`
//! already names this same set as `MissingFile` and its own doc comment says
//! plainly "the actual download runner is a later commit" — this module is
//! that commit.
//!
//! Nothing here infers connectivity itself
//! (`reprise_core::connectivity`'s module docs rule that out deliberately) —
//! the caller decides it flipped to online and calls this; offline is
//! therefore a plain no-op rather than a partial attempt.

use std::path::Path;

use rusqlite::Connection;

use crate::connectivity::Connectivity;
use crate::db::Db;

use super::download_state::DownloadState;
use super::pipeline::{download_episode, FeedFetcher, PipelineError, YoutubeFetcher};

/// Episode ids that are `wanted_on_device` (`MTP-40`) but still missing a
/// local file, in the stable order they were requested. Episode ids are
/// assigned in insertion order and a want is never reordered once set, so
/// ascending id is the same order the episodes were asked for.
pub fn pending_episode_ids(db: &Db) -> Result<Vec<i64>, rusqlite::Error> {
    let conn = db.conn();
    pending_episode_ids_in(conn)
}

pub(crate) fn pending_episode_ids_in(conn: &Connection) -> Result<Vec<i64>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT id FROM podcast_episodes
         WHERE wanted_on_device = 1 AND downloaded_path IS NULL
         ORDER BY id ASC",
    )?;
    let ids = statement
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<i64>, _>>()?;
    Ok(ids)
}

/// `NET-3c`: replays every pending episode's download, in order, once
/// `connectivity` is online — trusted as given, never re-derived. Returns
/// the episode ids that actually ran (in the order they ran), so a caller
/// can react per-episode without re-querying.
///
/// An episode that vanished between selection and running (removed or
/// unsubscribed concurrently) is skipped rather than aborting every
/// episode still waiting behind it; any other error stops the run so it is
/// not silently swallowed.
pub fn run_queued_downloads(
    db: &Db,
    feed_fetcher: &dyn FeedFetcher,
    youtube_fetcher: &dyn YoutubeFetcher,
    download_root: &Path,
    connectivity: Connectivity,
    mut on_progress: impl FnMut(i64, DownloadState),
) -> Result<Vec<i64>, PipelineError> {
    let conn = db.conn();
    if connectivity.is_offline() {
        return Ok(Vec::new());
    }
    let pending = pending_episode_ids_in(conn)?;
    let mut ran = Vec::with_capacity(pending.len());
    for episode_id in pending {
        match download_episode(
            db,
            feed_fetcher,
            youtube_fetcher,
            download_root,
            episode_id,
            &mut |state| on_progress(episode_id, state),
        ) {
            Ok(_) => ran.push(episode_id),
            Err(PipelineError::EpisodeNotFound) => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(ran)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::path::Path;

    use super::*;
    use crate::podcasts::feed::ParsedEpisode;
    use crate::podcasts::store::{self, NewSubscription};
    use crate::podcasts::{PodcastError, PodcastKind};

    #[derive(Default)]
    struct RecordingFeed {
        downloads: RefCell<Vec<String>>,
    }

    impl FeedFetcher for RecordingFeed {
        fn fetch(
            &self,
            _: &crate::podcasts::SubscriptionRow,
        ) -> Result<crate::podcasts::http::Response, PodcastError> {
            unreachable!("these tests only download, never refresh")
        }

        fn download(&self, url: &str, destination: &Path) -> Result<(), PodcastError> {
            self.downloads.borrow_mut().push(url.to_owned());
            std::fs::write(destination, b"audio")
                .map_err(|error| PodcastError::Body(error.to_string()))
        }
    }

    #[derive(Default)]
    struct NeverYoutube;

    impl YoutubeFetcher for NeverYoutube {
        fn list(
            &self,
            _: &str,
            _: usize,
        ) -> Result<crate::podcasts::feed::ParsedFeed, PodcastError> {
            unreachable!("these tests never touch YouTube")
        }

        fn download(&self, _: &str, _: &Path) -> Result<(), PodcastError> {
            unreachable!("these tests never touch YouTube")
        }
    }

    fn conn() -> Db {
        let db = Db::open_in_memory().unwrap();
        crate::modules::set_enabled(&db, &crate::modules::PODCASTS_MODULE, true).unwrap();
        db
    }

    fn subscription(conn: &Connection) -> i64 {
        store::add_or_restore_in(
            conn,
            &NewSubscription {
                kind: PodcastKind::Rss,
                feed_url: "https://example.test/feed".to_owned(),
                title: "Show".to_owned(),
                author: None,
                image_url: None,
                auto_download: false,
            },
            1,
        )
        .unwrap()
    }

    fn wanted_episode(conn: &Connection, subscription_id: i64, guid: &str) -> i64 {
        let episode_id = store::upsert_episode_in(
            conn,
            subscription_id,
            &ParsedEpisode {
                guid: guid.to_owned(),
                title: format!("Episode {guid}"),
                audio_url: format!("https://example.test/{guid}.mp3"),
                page_url: None,
                published_at: None,
                duration_secs: None,
            },
            1,
        )
        .unwrap()
        .unwrap()
        .episode_id;
        crate::podcasts::wanted_on_device::set_wanted_on_device_in(conn, episode_id, true).unwrap();
        episode_id
    }

    #[test]
    fn net_3c_offline_runs_nothing_and_leaves_the_queue_untouched() {
        let conn = conn();
        let subscription_id = subscription(conn.conn());
        let episode_id = wanted_episode(conn.conn(), subscription_id, "a");
        let directory = tempfile::tempdir().unwrap();
        let feed = RecordingFeed::default();

        let ran = run_queued_downloads(
            &conn,
            &feed,
            &NeverYoutube,
            directory.path(),
            Connectivity::Offline,
            |_, _| {},
        )
        .unwrap();

        assert!(ran.is_empty());
        assert!(
            feed.downloads.borrow().is_empty(),
            "offline must not touch the network at all, not even attempt and fail"
        );
        assert_eq!(
            pending_episode_ids(&conn).unwrap(),
            vec![episode_id],
            "an offline run must not consume the pending queue"
        );
    }

    /// The central F2 claim: a queued action actually runs once connectivity
    /// returns — a real file lands on disk and the DB row updates, not just
    /// a state flag flipping in memory.
    #[test]
    fn net_3c_a_queued_download_actually_runs_once_connectivity_returns() {
        let conn = conn();
        let subscription_id = subscription(conn.conn());
        let episode_id = wanted_episode(conn.conn(), subscription_id, "a");
        let directory = tempfile::tempdir().unwrap();
        let feed = RecordingFeed::default();

        let ran = run_queued_downloads(
            &conn,
            &feed,
            &NeverYoutube,
            directory.path(),
            Connectivity::Online,
            |_, _| {},
        )
        .unwrap();

        assert_eq!(ran, vec![episode_id]);
        assert_eq!(
            feed.downloads.borrow().as_slice(),
            ["https://example.test/a.mp3".to_owned()]
        );
        let row = store::episode(&conn, episode_id).unwrap().unwrap();
        assert!(
            row.downloaded_path.is_some(),
            "a real file must be recorded, not merely a state transition"
        );
        assert_eq!(row.downloaded_bytes, Some(5));
        assert_eq!(
            pending_episode_ids(&conn).unwrap(),
            Vec::<i64>::new(),
            "a downloaded episode must leave the pending queue"
        );
    }

    #[test]
    fn net_3c_replays_pending_downloads_in_the_order_they_were_requested() {
        let conn = conn();
        let subscription_id = subscription(conn.conn());
        // "b" is requested (inserted and marked wanted) before "a" gets its
        // own row, so a wrong implementation that replayed alphabetically or
        // in reverse would still be caught here.
        let episode_b = wanted_episode(conn.conn(), subscription_id, "b");
        let episode_a = wanted_episode(conn.conn(), subscription_id, "a");
        let directory = tempfile::tempdir().unwrap();
        let feed = RecordingFeed::default();

        let ran = run_queued_downloads(
            &conn,
            &feed,
            &NeverYoutube,
            directory.path(),
            Connectivity::Online,
            |_, _| {},
        )
        .unwrap();

        assert_eq!(
            ran,
            vec![episode_b, episode_a],
            "pending episodes replay in ascending id order, i.e. request order"
        );
        assert_eq!(
            feed.downloads.borrow().as_slice(),
            [
                "https://example.test/b.mp3".to_owned(),
                "https://example.test/a.mp3".to_owned(),
            ]
        );
    }

    #[test]
    fn net_3c_an_already_local_episode_is_never_replayed() {
        let conn = conn();
        let subscription_id = subscription(conn.conn());
        let episode_id = wanted_episode(conn.conn(), subscription_id, "a");
        let directory = tempfile::tempdir().unwrap();
        let feed = RecordingFeed::default();
        run_queued_downloads(
            &conn,
            &feed,
            &NeverYoutube,
            directory.path(),
            Connectivity::Online,
            |_, _| {},
        )
        .unwrap();
        assert_eq!(feed.downloads.borrow().len(), 1);

        let ran = run_queued_downloads(
            &conn,
            &feed,
            &NeverYoutube,
            directory.path(),
            Connectivity::Online,
            |_, _| {},
        )
        .unwrap();

        assert!(
            ran.is_empty(),
            "an episode already downloaded on a previous run must not run again"
        );
        assert_eq!(feed.downloads.borrow().len(), 1);
        let _ = episode_id;
    }
}
