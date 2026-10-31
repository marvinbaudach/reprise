//! A Deezer photo whose download is refused (NET-9a).

use super::{cache, fetch, sidecar, PortraitError, PortraitOutcome};
use crate::musicbrainz::FetchError;
use std::cell::Cell;

const SEARCH: &str = r#"{"data":[{"id":7,"name":"Band","nb_fan":2,"picture_xl":"https://cdn-images.dzcdn.net/images/artist/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/1000x1000.jpg"}]}"#;
const DEEZER_IMAGE: &str =
    "https://cdn-images.dzcdn.net/images/artist/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/1000x1000.jpg";
const COMMONS_IMAGE: &str = "https://upload.wikimedia.org/portrait.jpg";
const NO_COVERS: &str = r#"{"data":[]}"#;
const RELATIONS: &str =
    r#"{"relations":[{"url":{"resource":"https://www.wikidata.org/wiki/Q42"}}]}"#;
const CLAIMS: &str = r#"{"claims":{"P18":[{"mainsnak":{"datavalue":{"value":"Portrait.jpg"}}}]}}"#;
const NO_CLAIMS: &str = r#"{"claims":{}}"#;
const INFO: &str = r#"{"query":{"pages":{"1":{"imageinfo":[{"thumburl":"https://upload.wikimedia.org/portrait.jpg","descriptionurl":"https://commons.wikimedia.org/wiki/File:Portrait.jpg","extmetadata":{"Artist":{"value":"Photo Name"},"LicenseShortName":{"value":"CC BY 4.0"},"LicenseUrl":{"value":"https://creativecommons.org/licenses/by/4.0"}}}]}}}}"#;

fn png_bytes() -> Vec<u8> {
    let mut buffer = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(1, 1)
        .write_to(&mut buffer, image::ImageFormat::Png)
        .unwrap();
    buffer.into_inner()
}

fn deezer(url: &str) -> Result<String, FetchError> {
    Ok(if url.contains("/albums?") {
        NO_COVERS
    } else {
        SEARCH
    }
    .to_owned())
}

/// Runs one fetch for "Band" whose Deezer image answers `deezer_image` and
/// whose Commons chain answers with `claims`. Returns the result and how many
/// Wikimedia requests were made.
fn fetch_with(
    dir: &std::path::Path,
    now: i64,
    deezer_image: FetchError,
    claims: &str,
) -> (Result<PortraitOutcome, PortraitError>, usize) {
    let wikimedia_calls = Cell::new(0);
    let mut deezer_text = deezer;
    let mut musicbrainz = |_: &str| Ok(RELATIONS.to_owned());
    let mut wikimedia_text = |url: &str| {
        wikimedia_calls.set(wikimedia_calls.get() + 1);
        Ok(if url.contains("wbgetclaims") {
            claims
        } else {
            INFO
        }
        .to_owned())
    };
    let mut deezer_image = Some(deezer_image);
    let mut image = |url: &str| match url {
        DEEZER_IMAGE => Err(deezer_image.take().unwrap_or(FetchError::Transport)),
        COMMONS_IMAGE => Ok(png_bytes()),
        other => panic!("unexpected image URL: {other}"),
    };
    let mut fetchers = fetch::Fetchers {
        deezer_text: &mut deezer_text,
        musicbrainz_text: &mut musicbrainz,
        wikimedia_text: &mut wikimedia_text,
        image: &mut image,
    };
    let result = fetch::load_or_fetch("Band", Some("mbid"), now, dir, &mut fetchers);
    (result, wikimedia_calls.get())
}

#[test]
fn net_9a_refused_deezer_image_falls_back_to_the_credited_commons_photo() {
    let dir = tempfile::tempdir().unwrap();

    let (result, _) = fetch_with(dir.path(), 1_000, FetchError::HttpStatus(403), CLAIMS);

    assert!(matches!(result, Ok(PortraitOutcome::Found(_))));
    assert_eq!(
        sidecar::load_credit(dir.path(), "Band").unwrap().author,
        "Photo Name"
    );
    assert!(!cache::negative_marker_path(dir.path(), "Band").exists());
}

#[test]
fn net_9a_refused_deezer_image_with_a_commons_miss_errors_without_a_negative_marker() {
    let dir = tempfile::tempdir().unwrap();

    let (result, wikimedia_calls) =
        fetch_with(dir.path(), 1_000, FetchError::HttpStatus(403), NO_CLAIMS);

    assert!(matches!(
        result,
        Err(PortraitError::Fetch(FetchError::HttpStatus(403)))
    ));
    assert!(
        wikimedia_calls > 0,
        "the Commons chain must have been asked"
    );
    assert!(!cache::negative_marker_path(dir.path(), "Band").exists());
}

#[test]
fn net_9a_refused_deezer_image_keeps_a_stale_portrait_without_asking_commons() {
    let dir = tempfile::tempdir().unwrap();
    let stale = cache::store_image(dir.path(), "Band", b"stale", "jpg").unwrap();
    let now = cache::file_epoch_secs(&stale) + 31 * 24 * 60 * 60;

    let (result, wikimedia_calls) =
        fetch_with(dir.path(), now, FetchError::HttpStatus(403), CLAIMS);

    assert!(matches!(result, Ok(PortraitOutcome::Found(path)) if path == stale));
    assert_eq!(wikimedia_calls, 0);
    assert!(!cache::negative_marker_path(dir.path(), "Band").exists());
}

#[test]
fn net_9a_transport_error_on_the_deezer_image_never_asks_commons() {
    let dir = tempfile::tempdir().unwrap();

    let (result, wikimedia_calls) = fetch_with(dir.path(), 1_000, FetchError::Transport, CLAIMS);

    assert!(matches!(
        result,
        Err(PortraitError::Fetch(FetchError::Transport))
    ));
    assert_eq!(wikimedia_calls, 0);
    assert!(!cache::negative_marker_path(dir.path(), "Band").exists());
}

/// A rate limit or a server error says nothing about this photo, so neither may
/// replace it with a Commons one.
fn assert_transient_status_never_asks_commons(status: u16) {
    let dir = tempfile::tempdir().unwrap();

    let (result, wikimedia_calls) =
        fetch_with(dir.path(), 1_000, FetchError::HttpStatus(status), CLAIMS);

    assert!(matches!(
        result,
        Err(PortraitError::Fetch(FetchError::HttpStatus(returned))) if returned == status
    ));
    assert_eq!(wikimedia_calls, 0);
    assert!(!cache::negative_marker_path(dir.path(), "Band").exists());
}

#[test]
fn net_9a_server_error_on_the_deezer_image_never_asks_commons() {
    assert_transient_status_never_asks_commons(500);
}

#[test]
fn net_9a_rate_limit_on_the_deezer_image_never_asks_commons() {
    assert_transient_status_never_asks_commons(429);
}

#[test]
fn net_9a_not_found_on_the_deezer_image_is_a_refusal_that_asks_commons() {
    let dir = tempfile::tempdir().unwrap();

    let (result, _) = fetch_with(dir.path(), 1_000, FetchError::HttpStatus(404), CLAIMS);

    assert!(matches!(result, Ok(PortraitOutcome::Found(_))));
}
