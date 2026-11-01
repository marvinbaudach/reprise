//! A stale portrait whose refresh fails keeps serving, and an artist-shaped failure
//! is remembered for a day so the next start stays quiet (NET-7e).

use std::fs::{File, FileTimes};
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use super::{cache, fetch, PortraitOutcome};
use crate::musicbrainz::FetchError;

const SEARCH: &str = r#"{"data":[{"id":7,"name":"Band","nb_fan":2,"picture_xl":"https://cdn-images.dzcdn.net/images/artist/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/1000x1000.jpg"}]}"#;
const STALE_DAYS: u64 = 40;
const DAY_SECONDS: u64 = 24 * 60 * 60;

fn backdated_portrait(dir: &Path, now: i64) -> PathBuf {
    let path = cache::store_image(dir, "Band", b"stale", "jpg").unwrap();
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(
            FileTimes::new().set_modified(
                UNIX_EPOCH + Duration::from_secs(now as u64 - STALE_DAYS * DAY_SECONDS),
            ),
        )
        .unwrap();
    path
}

fn refresh_failing_with(
    dir: &Path,
    now: i64,
    album_error: FetchError,
) -> Result<PortraitOutcome, super::PortraitError> {
    let mut album_error = Some(album_error);
    let mut deezer_text = |url: &str| {
        if url.contains("/albums?") {
            Err(album_error.take().unwrap())
        } else {
            Ok(SEARCH.to_owned())
        }
    };
    let mut unused_musicbrainz = |_: &str| panic!("must stop at the album failure");
    let mut unused_wikimedia = |_: &str| panic!("must stop at the album failure");
    let mut image = |_: &str| panic!("must not download");
    let mut fetchers = fetch::Fetchers {
        deezer_text: &mut deezer_text,
        musicbrainz_text: &mut unused_musicbrainz,
        wikimedia_text: &mut unused_wikimedia,
        image: &mut image,
    };
    fetch::load_or_fetch("Band", None, now, dir, &mut fetchers)
}

#[test]
fn net_7e_a_stale_portrait_whose_refresh_is_rejected_is_not_asked_again_for_a_day() {
    let dir = tempfile::tempdir().unwrap();
    let now = chrono::Utc::now().timestamp();
    let stale = backdated_portrait(dir.path(), now);
    assert!(cache::verdict(dir.path(), "Band", now).needs_fetch());

    let outcome = refresh_failing_with(dir.path(), now, FetchError::HttpStatus(403)).unwrap();

    assert!(matches!(outcome, PortraitOutcome::Found(path) if path == stale));
    assert!(!cache::verdict(dir.path(), "Band", now).needs_fetch());
    assert!(cache::retry_marker_path(dir.path(), "Band").exists());
    assert!(!cache::negative_marker_path(dir.path(), "Band").exists());
    let mut no_deezer = |_: &str| panic!("a remembered rejection must not ask again");
    let mut no_musicbrainz = |_: &str| panic!("a remembered rejection must not ask again");
    let mut no_wikimedia = |_: &str| panic!("a remembered rejection must not ask again");
    let mut no_image = |_: &str| panic!("a remembered rejection must not ask again");
    let mut fetchers = fetch::Fetchers {
        deezer_text: &mut no_deezer,
        musicbrainz_text: &mut no_musicbrainz,
        wikimedia_text: &mut no_wikimedia,
        image: &mut no_image,
    };
    let again = fetch::load_or_fetch("Band", None, now, dir.path(), &mut fetchers).unwrap();
    assert!(matches!(again, PortraitOutcome::Found(path) if path == stale));
}

#[test]
fn net_7e_a_stale_portrait_whose_refresh_hits_the_network_is_asked_again_next_start() {
    let dir = tempfile::tempdir().unwrap();
    let now = chrono::Utc::now().timestamp();
    let stale = backdated_portrait(dir.path(), now);

    let outcome = refresh_failing_with(dir.path(), now, FetchError::Transport).unwrap();

    assert!(matches!(outcome, PortraitOutcome::Found(path) if path == stale));
    assert!(cache::verdict(dir.path(), "Band", now).needs_fetch());
    assert!(!cache::retry_marker_path(dir.path(), "Band").exists());
    assert!(!cache::negative_marker_path(dir.path(), "Band").exists());
}
