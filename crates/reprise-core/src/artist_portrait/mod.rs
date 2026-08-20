//! Artist portraits are fetched from Deezer after a library scan or restore, which sends the
//! library's artist names to Deezer.
//! Blocking; call from a worker thread.

mod cache;
pub(crate) mod deezer;
mod placeholder;

#[cfg(test)]
mod placeholder_measurement;
#[cfg(test)]
mod test_fixtures;

pub(crate) use cache::{cache_dir, IMAGE_EXTS};
pub use cache::{verdict, CacheVerdict};

use std::path::{Path, PathBuf};

use crate::{
    musicbrainz::FetchError,
    source_error::{SourceError, SourceErrorKind},
};

#[derive(Debug)]
pub enum PortraitOutcome {
    Found(PathBuf),
    NotFound,
}

#[derive(Debug, thiserror::Error)]
pub enum PortraitError {
    #[error(transparent)]
    Fetch(#[from] FetchError),
    #[error("Deezer response was invalid")]
    InvalidResponse,
}

impl From<&PortraitError> for SourceErrorKind {
    fn from(error: &PortraitError) -> Self {
        match error {
            PortraitError::Fetch(error) => Self::from(error),
            PortraitError::InvalidResponse => Self::Unreachable,
        }
    }
}

impl From<PortraitError> for SourceError {
    fn from(error: PortraitError) -> Self {
        let kind = SourceErrorKind::from(&error);
        Self::new(kind, "artist portrait request failed", error.to_string())
    }
}

pub fn load_or_fetch(name: &str) -> Result<PortraitOutcome, PortraitError> {
    load_or_fetch_in(name, &cache::cache_dir())
}

pub fn load_or_fetch_in(name: &str, dir: &Path) -> Result<PortraitOutcome, PortraitError> {
    let now = chrono::Utc::now().timestamp();
    load_or_fetch_with(
        name,
        now,
        dir,
        &mut deezer::search,
        &mut deezer::download_image,
    )
}

/// Resolves only an already-downloaded portrait and never contacts Deezer.
pub fn load_cached(name: &str) -> PortraitOutcome {
    load_cached_from(name, &cache::cache_dir())
}

/// Resolves an already-downloaded portrait from an explicit cache directory.
///
/// Frontend tests use this variant with a temporary directory so portrait
/// rendering never consults the user's cache or contacts the network.
pub fn load_cached_from(name: &str, dir: &Path) -> PortraitOutcome {
    let name = name.trim();
    if name.is_empty() {
        return PortraitOutcome::NotFound;
    }
    cache::portrait_path_in(dir, name).map_or(PortraitOutcome::NotFound, PortraitOutcome::Found)
}

/// Stores a portrait fixture through the production cache writer.
///
/// Cross-crate tests compile `reprise-core` as a dependency, so `cfg(test)` is
/// not active there. Debug assertions keep this seam out of release builds
/// while making it available to those tests.
#[cfg(debug_assertions)]
#[doc(hidden)]
pub fn store_fixture_image(
    dir: &Path,
    name: &str,
    bytes: &[u8],
    extension: &str,
) -> Option<PathBuf> {
    cache::store_image(dir, name, bytes, extension)
}

pub(crate) fn load_or_fetch_with<S, D>(
    name: &str,
    now: i64,
    dir: &Path,
    search: &mut S,
    download: &mut D,
) -> Result<PortraitOutcome, PortraitError>
where
    S: FnMut(&str) -> Result<String, FetchError>,
    D: FnMut(&str) -> Result<Vec<u8>, FetchError>,
{
    let name = name.trim();
    if name.is_empty() {
        return Ok(PortraitOutcome::NotFound);
    }

    let cached_path = match cache::verdict(dir, name, now) {
        cache::CacheVerdict::FreshPortrait(path) => return Ok(PortraitOutcome::Found(path)),
        cache::CacheVerdict::FreshNegative => return Ok(PortraitOutcome::NotFound),
        cache::CacheVerdict::NeedsFetch { stale_portrait } => stale_portrait,
    };

    let body = match search(&deezer::search_url(name)) {
        Ok(body) => body,
        Err(error) => return stale_or(cached_path, error.into()),
    };
    let Some(artist) = deezer::parse_best_artist(&body, name) else {
        cache::write_negative(dir, name);
        return Ok(PortraitOutcome::NotFound);
    };
    let Some(url) = artist.picture_url else {
        cache::write_negative(dir, name);
        return Ok(PortraitOutcome::NotFound);
    };
    let bytes = match download(&url) {
        Ok(bytes) => bytes,
        Err(error) => return stale_or(cached_path, error.into()),
    };
    let decoded_image = crate::cover_download::decode_image(&bytes);
    let placeholder_distance = decoded_image
        .as_ref()
        .map(|decoded| placeholder::placeholder_distance(decoded.image()));
    if let Some(distance) =
        placeholder_distance.filter(|distance| *distance <= placeholder::PLACEHOLDER_RMSE_MAX)
    {
        let image_identifier = deezer::image_identifier(&url).unwrap_or_else(|| "unknown".into());
        tracing::warn!(
            artist = name,
            image_identifier,
            placeholder_distance = distance,
            "artist portrait rejected as a known Deezer placeholder"
        );
        if let Some(path) = cached_path.as_ref().filter(|path| path.exists()) {
            let refreshed = cache::refresh_image(dir, name, path).unwrap_or_else(|| {
                tracing::warn!(
                    artist = name,
                    cached_path = %path.display(),
                    "artist portrait could not refresh the cached image after placeholder rejection"
                );
                path.clone()
            });
            return Ok(PortraitOutcome::Found(refreshed));
        }
        cache::write_negative(dir, name);
        return Ok(PortraitOutcome::NotFound);
    }
    let Some(extension) = decoded_image
        .as_ref()
        .and_then(crate::cover_download::DecodedImage::validated_extension)
    else {
        cache::write_negative(dir, name);
        return Ok(PortraitOutcome::NotFound);
    };
    if let Some(distance) = placeholder_distance
        .filter(|distance| *distance < placeholder::PLACEHOLDER_WARNING_RMSE_MAX)
    {
        let image_identifier = deezer::image_identifier(&url).unwrap_or_else(|| "unknown".into());
        tracing::warn!(
            artist = name,
            image_identifier,
            placeholder_distance = distance,
            "artist portrait accepted near the placeholder threshold"
        );
    }
    cache::store_image(dir, name, &bytes, extension)
        .map(PortraitOutcome::Found)
        .map_or_else(|| stale_or(cached_path, PortraitError::InvalidResponse), Ok)
}

pub(crate) fn normalize(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn stale_or(
    cached_path: Option<PathBuf>,
    error: PortraitError,
) -> Result<PortraitOutcome, PortraitError> {
    cached_path
        .filter(|path| path.exists())
        .map(PortraitOutcome::Found)
        .ok_or(error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artist_portrait::test_fixtures::{
        ALL_PLACEHOLDERS_RESPONSE, MISSING_FANS_RESPONSE, NON_EXACT_RESPONSE, OCEANO_RESPONSE,
        ONI_RESPONSE, POPULAR_PLACEHOLDER_RESPONSE, THE_DEVIL_WEARS_PRADA_RESPONSE,
    };
    use crate::musicbrainz::FetchError;
    use crate::source_error::{SourceError, SourceErrorKind};

    #[test]
    fn portrait_failures_project_without_displaying_the_http_status() {
        let error = SourceError::from(PortraitError::Fetch(FetchError::HttpStatus(599)));

        assert_eq!(error.kind(), &SourceErrorKind::Unreachable);
        assert!(!error.to_string().contains("599"));
        assert!(error
            .details("2026-07-30 14:12")
            .to_string()
            .contains("HTTP status 599"));
    }

    fn png_bytes() -> Vec<u8> {
        let mut buffer = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(1, 1)
            .write_to(&mut buffer, image::ImageFormat::Png)
            .unwrap();
        buffer.into_inner()
    }

    fn placeholder_png(reference_index: usize) -> Vec<u8> {
        let reference = image::GrayImage::from_raw(
            32,
            32,
            placeholder::REFERENCE_THUMBNAILS[reference_index].to_vec(),
        )
        .unwrap();
        let mut buffer = std::io::Cursor::new(Vec::new());
        image::DynamicImage::from(reference)
            .write_to(&mut buffer, image::ImageFormat::Png)
            .unwrap();
        buffer.into_inner()
    }

    fn near_placeholder_png() -> Vec<u8> {
        let shifted = placeholder::REFERENCE_THUMBNAILS[0].map(|luma| luma.saturating_add(3));
        let image = image::GrayImage::from_raw(32, 32, shifted.to_vec()).unwrap();
        let mut buffer = std::io::Cursor::new(Vec::new());
        image::DynamicImage::from(image)
            .write_to(&mut buffer, image::ImageFormat::Png)
            .unwrap();
        buffer.into_inner()
    }

    const HIT: &str = r#"{"data":[{"id":1,"name":"Band","nb_album":1,"nb_fan":1,"picture_xl":"https://cdn-images.dzcdn.net/images/artist/abc/1000x1000-000000-80-0-0.jpg","picture_big":"https://cdn-images.dzcdn.net/images/artist/abc/500x500-000000-80-0-0.jpg","type":"artist"}],"total":1}"#;

    fn tmp() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("rp-portrait-mod-{}", fastrand::u64(..)))
    }

    #[test]
    fn load_or_fetch_in_answers_a_fresh_portrait_from_the_given_directory() {
        let dir = tmp();
        let name = format!("Reprise Fixture {}", fastrand::u64(..));
        cache::store_image(&dir, &name, b"img", "jpg").unwrap();

        let outcome = load_or_fetch_in(&name, &dir).unwrap();

        match outcome {
            PortraitOutcome::Found(path) => assert!(path.starts_with(&dir)),
            PortraitOutcome::NotFound => panic!("expected Found"),
        }
        assert!(cache::portrait_path_in(&cache::cache_dir(), &name).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_or_fetch_in_honours_a_negative_marker_in_the_given_directory() {
        let dir = tmp();
        let name = format!("Reprise Fixture {}", fastrand::u64(..));
        cache::write_negative(&dir, &name);

        let outcome = load_or_fetch_in(&name, &dir).unwrap();

        assert!(matches!(outcome, PortraitOutcome::NotFound));
        assert!(!cache::negative_marker_path(&cache::cache_dir(), &name).exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn found_downloads_and_stores_image() {
        let dir = tmp();
        let mut search = |_: &str| Ok(HIT.to_string());
        let mut download = |_: &str| Ok(png_bytes());
        let outcome = load_or_fetch_with("Band", 1_000, &dir, &mut search, &mut download).unwrap();
        match outcome {
            PortraitOutcome::Found(path) => assert!(path.exists()),
            PortraitOutcome::NotFound => panic!("expected Found"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn downloaded_invalid_image_writes_a_negative_outcome() {
        let dir = tmp();
        let mut search = |_: &str| Ok(HIT.to_string());
        let mut download = |_: &str| Ok(b"not an image".to_vec());

        let outcome = load_or_fetch_with("Band", 1_000, &dir, &mut search, &mut download).unwrap();

        assert!(matches!(outcome, PortraitOutcome::NotFound));
        assert!(cache::portrait_path_in(&dir, "Band").is_none());
        assert!(cache::negative_marker_path(&dir, "Band").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn downloaded_placeholder_without_cached_portrait_writes_one_negative_outcome() {
        let dir = tmp();
        let logs = crate::log_capture::CapturedLogs::default();
        let downloads = std::cell::Cell::new(0);
        let mut search = |_: &str| Ok(HIT.to_string());
        let mut download = |_: &str| {
            downloads.set(downloads.get() + 1);
            Ok(placeholder_png(0))
        };

        let outcome = logs
            .capture(|| load_or_fetch_with("Band", 1_000, &dir, &mut search, &mut download))
            .unwrap();

        assert!(matches!(outcome, PortraitOutcome::NotFound));
        assert_eq!(downloads.get(), 1);
        assert!(cache::portrait_path_in(&dir, "Band").is_none());
        assert!(cache::negative_marker_path(&dir, "Band").exists());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        let logs = logs.joined();
        assert!(logs.contains("artist portrait rejected as a known Deezer placeholder"));
        assert!(logs.contains("Band"));
        assert!(logs.contains("abc"));
        assert!(logs.contains("placeholder_distance"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn downloaded_placeholder_refreshes_and_preserves_a_stale_cached_portrait() {
        let dir = tmp();
        let cached = cache::store_image(&dir, "Band", b"existing portrait", "jpg").unwrap();
        let before_modified = std::fs::metadata(&cached).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1_100));
        let stale_now = cache::file_epoch_secs(&cached) + 31 * 24 * 60 * 60;
        let downloads = std::cell::Cell::new(0);
        let mut search = |_: &str| Ok(HIT.to_string());
        let mut download = |_: &str| {
            downloads.set(downloads.get() + 1);
            Ok(placeholder_png(0))
        };

        let outcome =
            load_or_fetch_with("Band", stale_now, &dir, &mut search, &mut download).unwrap();

        match outcome {
            PortraitOutcome::Found(path) => assert_eq!(path, cached),
            PortraitOutcome::NotFound => panic!("expected the cached portrait"),
        }
        assert_eq!(downloads.get(), 1);
        assert_eq!(std::fs::read(&cached).unwrap(), b"existing portrait");
        assert!(std::fs::metadata(&cached).unwrap().modified().unwrap() > before_modified);
        assert!(!cache::negative_marker_path(&dir, "Band").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn failed_cached_portrait_refresh_emits_a_warning() {
        let dir = tmp();
        let cached = cache::store_image(&dir, "Band", b"existing portrait", "jpg").unwrap();
        let stale_now = cache::file_epoch_secs(&cached) + 31 * 24 * 60 * 60;
        let logs = crate::log_capture::CapturedLogs::default();
        let mut search = |_: &str| Ok(HIT.to_string());
        let mut download = |_: &str| {
            std::fs::remove_file(&cached).unwrap();
            std::fs::create_dir(&cached).unwrap();
            Ok(placeholder_png(0))
        };

        let outcome = logs
            .capture(|| load_or_fetch_with("Band", stale_now, &dir, &mut search, &mut download))
            .unwrap();

        match outcome {
            PortraitOutcome::Found(path) => assert_eq!(path, cached),
            PortraitOutcome::NotFound => panic!("expected the stale cache path"),
        }
        let logs = logs.joined();
        assert!(logs.contains("artist portrait could not refresh the cached image"));
        assert!(logs.contains("Band"));
        assert!(logs.contains("cached_path"));
        assert!(logs.contains(&cached.display().to_string()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn accepted_image_near_the_placeholder_threshold_emits_a_warning() {
        let dir = tmp();
        let logs = crate::log_capture::CapturedLogs::default();
        let mut search = |_: &str| Ok(HIT.to_string());
        let mut download = |_: &str| Ok(near_placeholder_png());

        let outcome = logs
            .capture(|| load_or_fetch_with("Band", 1_000, &dir, &mut search, &mut download))
            .unwrap();

        assert!(matches!(outcome, PortraitOutcome::Found(_)));
        let logs = logs.joined();
        assert!(logs.contains("artist portrait accepted near the placeholder threshold"));
        assert!(logs.contains("Band"));
        assert!(logs.contains("abc"));
        assert!(logs.contains("placeholder_distance"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn all_exact_placeholder_matches_write_marker_without_download() {
        let dir = tmp();
        let mut search = |_: &str| Ok(ALL_PLACEHOLDERS_RESPONSE.to_string());
        let mut download = |_: &str| -> Result<Vec<u8>, FetchError> { panic!("must not download") };
        let outcome = load_or_fetch_with("Band", 1_000, &dir, &mut search, &mut download).unwrap();
        assert!(matches!(outcome, PortraitOutcome::NotFound));
        assert!(cache::negative_marker_path(&dir, "Band").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    fn fetched_image_identifier(json: &str, name: &str) -> String {
        let dir = tmp();
        let mut downloaded = Vec::new();
        let mut search = |url: &str| {
            assert!(url.contains("limit=10"));
            Ok(json.to_owned())
        };
        let mut download = |url: &str| {
            downloaded.push(url.to_owned());
            Ok(png_bytes())
        };

        let outcome = load_or_fetch_with(name, 1_000, &dir, &mut search, &mut download).unwrap();

        match outcome {
            PortraitOutcome::Found(path) => assert!(path.exists()),
            PortraitOutcome::NotFound => panic!("expected Found"),
        }
        assert!(!cache::negative_marker_path(&dir, name).exists());
        assert_eq!(downloaded.len(), 1);
        let url = url::Url::parse(&downloaded[0]).unwrap();
        assert!(url
            .host_str()
            .is_some_and(|host| host.ends_with(".dzcdn.net")));
        let identifier = url
            .path_segments()
            .and_then(|mut segments| segments.nth(2))
            .unwrap()
            .to_owned();
        std::fs::remove_dir_all(&dir).ok();
        identifier
    }

    #[test]
    fn devil_wears_prada_downloads_real_match_after_placeholder() {
        assert_eq!(
            fetched_image_identifier(THE_DEVIL_WEARS_PRADA_RESPONSE, "The Devil Wears Prada"),
            "ce8738d500000000000000000000c62a"
        );
    }

    #[test]
    fn oceano_selects_the_most_popular_exact_match_before_image_validation() {
        assert_eq!(
            fetched_image_identifier(OCEANO_RESPONSE, "Oceano"),
            "415714b66a5de709809dd3d05f58afe4"
        );
    }

    #[test]
    fn oceano_placeholder_is_rejected_without_trying_the_lower_ranked_namesake() {
        let dir = tmp();
        let downloaded = std::cell::RefCell::new(Vec::new());
        let mut search = |_: &str| Ok(OCEANO_RESPONSE.to_string());
        let mut download = |url: &str| {
            downloaded.borrow_mut().push(url.to_owned());
            Ok(placeholder_png(1))
        };

        let outcome =
            load_or_fetch_with("Oceano", 1_000, &dir, &mut search, &mut download).unwrap();

        assert!(matches!(outcome, PortraitOutcome::NotFound));
        let downloaded = downloaded.into_inner();
        assert_eq!(downloaded.len(), 1);
        assert!(downloaded[0].contains("/415714b66a5de709809dd3d05f58afe4/"));
        assert!(cache::negative_marker_path(&dir, "Oceano").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn oni_downloads_most_popular_exact_match_even_when_it_is_last() {
        assert_eq!(
            fetched_image_identifier(ONI_RESPONSE, "ONI"),
            "0a110000000000000000000000002558"
        );
    }

    #[test]
    fn non_exact_name_never_wins_even_with_many_more_fans() {
        assert_eq!(
            fetched_image_identifier(NON_EXACT_RESPONSE, "The Devil Wears Prada"),
            "ce8738d500000000000000000000c62a"
        );
    }

    #[test]
    fn real_image_outranks_a_more_popular_placeholder() {
        assert_eq!(
            fetched_image_identifier(POPULAR_PLACEHOLDER_RESPONSE, "Band"),
            "baad0000000000000000000000000001"
        );
    }

    #[test]
    fn missing_and_null_fan_counts_choose_stably_without_panicking() {
        let first = fetched_image_identifier(MISSING_FANS_RESPONSE, "Band");
        let second = fetched_image_identifier(MISSING_FANS_RESPONSE, "Band");

        assert_eq!(first, "baad0000000000000000000000000001");
        assert_eq!(second, first);
    }

    #[test]
    fn fresh_cached_image_short_circuits_without_fetch() {
        let dir = tmp();
        cache::store_image(&dir, "Band", b"img", "jpg").unwrap();
        let now = cache::file_epoch_secs(&cache::portrait_path_in(&dir, "Band").unwrap());
        let mut search = |_: &str| -> Result<String, FetchError> { panic!("must not search") };
        let mut download = |_: &str| -> Result<Vec<u8>, FetchError> { panic!("must not download") };
        let outcome = load_or_fetch_with("Band", now, &dir, &mut search, &mut download).unwrap();
        assert!(matches!(outcome, PortraitOutcome::Found(_)));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn net_1a_portraits_keep_cached_images_when_disabled() {
        let dir = tmp();
        let cached = cache::store_image(&dir, "Band", b"img", "jpg").unwrap();

        let outcome = load_cached_from("Band", &dir);

        match outcome {
            PortraitOutcome::Found(path) => assert_eq!(path, cached),
            PortraitOutcome::NotFound => panic!("expected cached portrait"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn fresh_negative_marker_short_circuits() {
        let dir = tmp();
        cache::write_negative(&dir, "Band");
        let now = cache::file_epoch_secs(&cache::negative_marker_path(&dir, "Band"));
        let mut search = |_: &str| -> Result<String, FetchError> { panic!("must not search") };
        let mut download = |_: &str| -> Result<Vec<u8>, FetchError> { panic!("must not download") };
        let outcome = load_or_fetch_with("Band", now, &dir, &mut search, &mut download).unwrap();
        assert!(matches!(outcome, PortraitOutcome::NotFound));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn stale_cached_image_survives_refresh_failure() {
        let dir = tmp();
        let cached = cache::store_image(&dir, "Band", b"old", "jpg").unwrap();
        let stale_now = cache::file_epoch_secs(&cached) + 31 * 24 * 60 * 60;
        let mut search = |_: &str| Err(FetchError::Transport);
        let mut download = |_: &str| -> Result<Vec<u8>, FetchError> { panic!("must not download") };

        let outcome =
            load_or_fetch_with("Band", stale_now, &dir, &mut search, &mut download).unwrap();

        match outcome {
            PortraitOutcome::Found(path) => assert_eq!(path, cached),
            PortraitOutcome::NotFound => panic!("expected stale cached portrait"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
