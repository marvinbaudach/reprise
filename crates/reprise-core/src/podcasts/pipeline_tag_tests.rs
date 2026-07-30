//! `POD-17`'s download-tagging tests, split out of
//! `pipeline_refresh_tests.rs` only to keep that file under the project's
//! 800-line rule — the same reason every other `_tests.rs` split in this
//! module exists. Declared inside `pipeline.rs`, so `super` is still that
//! module and every test reads exactly as it did there.

use std::path::PathBuf;

use super::*;
use crate::podcasts::store::{self, NewSubscription};

/// Serves the real FLAC fixture, so the tag write has a container it can
/// actually rewrite.
struct FixtureAudioFeed;

impl FeedFetcher for FixtureAudioFeed {
    fn fetch(&self, _: &SubscriptionRow) -> Result<Response, PodcastError> {
        unreachable!("download_episode never refreshes the feed")
    }

    fn download(&self, _: &str, destination: &Path) -> Result<(), PodcastError> {
        std::fs::copy(fixture(), destination)
            .map(|_| ())
            .map_err(|error| PodcastError::Body(error.to_string()))
    }
}

/// Serves five bytes that are no audio container at all: the `POD-17` case
/// where the tag write is refused before anything is written.
struct UntaggableFeed;

impl FeedFetcher for UntaggableFeed {
    fn fetch(&self, _: &SubscriptionRow) -> Result<Response, PodcastError> {
        unreachable!("download_episode never refreshes the feed")
    }

    fn download(&self, _: &str, destination: &Path) -> Result<(), PodcastError> {
        std::fs::write(destination, b"audio").map_err(|error| PodcastError::Body(error.to_string()))
    }
}

/// Serves the FLAC fixture and then takes write permission away from it, so
/// the tag write fails at the `open(O_RDWR)` that precedes lofty's truncate.
struct ReadOnlyAudioFeed;

impl FeedFetcher for ReadOnlyAudioFeed {
    fn fetch(&self, _: &SubscriptionRow) -> Result<Response, PodcastError> {
        unreachable!("download_episode never refreshes the feed")
    }

    fn download(&self, _: &str, destination: &Path) -> Result<(), PodcastError> {
        std::fs::copy(fixture(), destination)
            .map_err(|error| PodcastError::Body(error.to_string()))?;
        set_read_only(destination);
        Ok(())
    }
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac")
}

fn set_read_only(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o444)).unwrap();
}

#[derive(Default)]
struct FakeYoutube;

impl YoutubeFetcher for FakeYoutube {
    fn list(&self, _: &str, _: usize) -> Result<ParsedFeed, PodcastError> {
        Err(PodcastError::YtDlp("unexpected YouTube call".to_owned()))
    }

    fn download(&self, _: &str, _: &Path) -> Result<(), PodcastError> {
        Err(PodcastError::YtDlp("unexpected YouTube call".to_owned()))
    }
}

fn conn() -> Db {
    let conn = Db::open_in_memory().unwrap();
    crate::modules::set_enabled(&conn, &crate::modules::PODCASTS_MODULE, true).unwrap();
    conn
}

fn add_downloadable_episode(db: &Db, audio_url: &str, published_at: Option<i64>) -> i64 {
    let subscription_id = store::add_or_restore(
        db,
        &NewSubscription {
            kind: PodcastKind::Rss,
            feed_url: "https://example.test/tagged-feed".to_owned(),
            title: "The Show".to_owned(),
            author: Some("The Author".to_owned()),
            image_url: None,
            auto_download: false,
        },
        1,
    )
    .unwrap();
    store::upsert_episode(
        db,
        subscription_id,
        &crate::podcasts::feed::ParsedEpisode {
            guid: "tagged-episode".to_owned(),
            title: "Episode title".to_owned(),
            image_url: None,
            audio_url: audio_url.to_owned(),
            page_url: None,
            published_at,
            duration_secs: None,
        },
        1_785_225_600,
    )
    .unwrap()
    .unwrap()
    .episode_id
}

/// Every regular file below `root`, so a test can assert that a failed
/// download left nothing at all behind — neither the temporary nor a
/// published file.
fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                pending.push(entry.path());
            } else {
                found.push(entry.path());
            }
        }
    }
    found
}

#[test]
fn pod_17_a_downloaded_episode_is_tagged_before_its_size_is_recorded() {
    use lofty::prelude::*;

    let db = conn();
    let episode_id = add_downloadable_episode(
        &db,
        "https://example.test/episode.flac",
        Some(1_785_225_600),
    );
    let directory = tempfile::tempdir().unwrap();
    let fixture_bytes = std::fs::metadata(fixture()).unwrap().len();
    let mut states = Vec::new();

    let outcome = download_episode(
        &db,
        &FixtureAudioFeed,
        &FakeYoutube,
        directory.path(),
        episode_id,
        &mut |state| states.push(state),
    )
    .unwrap();

    let stored = store::episode(&db, episode_id).unwrap().unwrap();
    let downloaded_path = Path::new(stored.downloaded_path.as_deref().unwrap());
    let tagged = lofty::read_from_path(downloaded_path).unwrap();
    let tag = tagged.primary_tag().unwrap();
    assert_eq!(tag.title().as_deref(), Some("Episode title"));
    assert_eq!(tag.album().as_deref(), Some("The Show"));
    assert_eq!(tag.artist().as_deref(), Some("The Author"));
    assert_eq!(
        tag.get_string(lofty::tag::ItemKey::AlbumArtist),
        Some("The Show")
    );
    assert_eq!(
        tag.get_string(lofty::tag::ItemKey::RecordingDate),
        Some("2026-07-28")
    );
    let published_bytes = std::fs::metadata(downloaded_path).unwrap().len();
    assert_eq!(stored.downloaded_bytes, Some(published_bytes as i64));
    assert_ne!(published_bytes, fixture_bytes);
    assert_eq!(
        outcome,
        DownloadState::Downloaded {
            bytes: published_bytes
        }
    );
    assert_eq!(
        states.last(),
        Some(&DownloadState::Downloaded {
            bytes: published_bytes
        })
    );
}

#[test]
fn pod_17_an_untaggable_download_is_still_published_with_its_true_size() {
    let db = conn();
    let episode_id = add_downloadable_episode(&db, "https://example.test/episode.mp3", None);
    let directory = tempfile::tempdir().unwrap();

    let outcome = download_episode(
        &db,
        &UntaggableFeed,
        &FakeYoutube,
        directory.path(),
        episode_id,
        &mut |_| {},
    )
    .unwrap();

    assert_eq!(outcome, DownloadState::Downloaded { bytes: 5 });
    let stored = store::episode(&db, episode_id).unwrap().unwrap();
    assert_eq!(stored.downloaded_bytes, Some(5));
    assert!(Path::new(stored.downloaded_path.as_deref().unwrap()).is_file());
}

/// `POD-17`: a *write* failure is the opposite case of the untaggable
/// container above, and collapsing the two is how a destroyed file used to
/// become a finished download. lofty rewrites Ogg and FLAC by truncating
/// first, so a rewrite that dies part-way (a full disk is the realistic
/// trigger) leaves a truncated file — and the size recorded for it would be
/// measured from exactly that wreckage, which makes it undetectable
/// afterwards: device sync only checks the file against that same number and
/// agrees forever, while a set `downloaded_path` stops the episode from ever
/// being downloaded again. So the download has to fail, which deletes the
/// `.part` and leaves the episode retryable.
///
/// The failure is simulated with a read-only temporary, which lofty refuses
/// at the `open(O_RDWR)` preceding its truncate. That leaves the file
/// intact, unlike the full-disk case — but the two are the same error to
/// every caller, so the rule cannot distinguish them either.
#[test]
fn pod_17_a_download_whose_tag_write_fails_is_never_published() {
    let db = conn();
    let episode_id = add_downloadable_episode(
        &db,
        "https://example.test/episode.flac",
        Some(1_785_225_600),
    );
    let directory = tempfile::tempdir().unwrap();
    let probe = directory.path().join("probe");
    std::fs::write(&probe, b"probe").unwrap();
    set_read_only(&probe);
    if std::fs::OpenOptions::new().write(true).open(&probe).is_ok() {
        // File permissions are not enforced here (e.g. running as root in a
        // container), so this failure cannot be simulated.
        eprintln!(
            "skipping pod_17_a_download_whose_tag_write_fails_is_never_published: \
             file permissions are not enforced in this environment"
        );
        return;
    }
    std::fs::remove_file(&probe).unwrap();

    let outcome = download_episode(
        &db,
        &ReadOnlyAudioFeed,
        &FakeYoutube,
        directory.path(),
        episode_id,
        &mut |_| {},
    )
    .unwrap();

    assert!(
        matches!(outcome, DownloadState::Failed { .. }),
        "a failed tag write must fail the download, not publish it: {outcome:?}"
    );
    let stored = store::episode(&db, episode_id).unwrap().unwrap();
    assert_eq!(
        stored.downloaded_path, None,
        "the episode must stay downloadable, not be recorded as complete"
    );
    assert_eq!(stored.downloaded_bytes, None);
    assert_eq!(
        files_under(directory.path()),
        Vec::<PathBuf>::new(),
        "neither the temporary nor a published file may survive"
    );
}

#[test]
fn pod_17_the_date_in_the_device_path_is_the_date_in_the_file_tag() {
    use lofty::prelude::*;

    let db = conn();
    crate::modules::set_enabled(&db, &crate::modules::YOUTUBE_MODULE, true).unwrap();
    let episode_id = add_downloadable_episode(
        &db,
        "https://example.test/episode.flac",
        Some(1_785_312_000),
    );
    let directory = tempfile::tempdir().unwrap();
    download_episode(
        &db,
        &FixtureAudioFeed,
        &FakeYoutube,
        directory.path(),
        episode_id,
        &mut |_| {},
    )
    .unwrap();
    let stored = store::episode(&db, episode_id).unwrap().unwrap();
    crate::podcasts::phone_sync::set_device_enabled(&db, stored.subscription_id, "mtp:pixel", true)
        .unwrap();

    let candidates =
        crate::device_sync::podcasts::query_candidates_for_device(&db, "mtp:pixel").unwrap();
    let file_name = candidates[0].device_path.rsplit('/').next().unwrap();
    let device_date = &file_name[..10];
    let tagged =
        lofty::read_from_path(Path::new(stored.downloaded_path.as_deref().unwrap())).unwrap();
    let tag_date = tagged
        .primary_tag()
        .unwrap()
        .get_string(lofty::tag::ItemKey::RecordingDate)
        .unwrap();

    assert_eq!(device_date, tag_date);
}
