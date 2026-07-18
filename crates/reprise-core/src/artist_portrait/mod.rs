//! Cached artist portraits from Deezer. Blocking; call from a worker thread.

pub(crate) mod cache;
pub(crate) mod deezer;

use std::path::{Path, PathBuf};

use crate::musicbrainz::FetchError;

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

pub fn load_or_fetch(name: &str, force: bool) -> Result<PortraitOutcome, PortraitError> {
    let now = chrono::Utc::now().timestamp();
    let dir = cache::cache_dir();
    load_or_fetch_with(
        name,
        force,
        now,
        &dir,
        &mut deezer::search,
        &mut deezer::download_image,
    )
}

pub(crate) fn load_or_fetch_with<S, D>(
    name: &str,
    force: bool,
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

    if !force {
        if let Some(path) = cache::portrait_path_in(dir, name) {
            if cache::is_fresh(cache::file_epoch_secs(&path), now, true) {
                return Ok(PortraitOutcome::Found(path));
            }
        }
        let marker = cache::negative_marker_path(dir, name);
        if marker.exists() && cache::is_fresh(cache::file_epoch_secs(&marker), now, false) {
            return Ok(PortraitOutcome::NotFound);
        }
    }

    let body = search(&deezer::search_url(name))?;
    let Some(artist) = deezer::parse_best_artist(&body, name) else {
        cache::write_negative(dir, name);
        return Ok(PortraitOutcome::NotFound);
    };
    debug_assert_eq!(cache::key_for(&artist.name), cache::key_for(name));
    let Some(url) = artist.picture_url else {
        cache::write_negative(dir, name);
        return Ok(PortraitOutcome::NotFound);
    };
    let bytes = download(&url)?;
    let Some(extension) = crate::cover_download::validated_image_extension(&bytes) else {
        cache::write_negative(dir, name);
        return Ok(PortraitOutcome::NotFound);
    };
    cache::store_image(dir, name, &bytes, extension)
        .map(PortraitOutcome::Found)
        .ok_or(PortraitError::InvalidResponse)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::musicbrainz::FetchError;

    fn png_bytes() -> Vec<u8> {
        let mut buffer = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(1, 1)
            .write_to(&mut buffer, image::ImageFormat::Png)
            .unwrap();
        buffer.into_inner()
    }

    const HIT: &str = r#"{"data":[{"id":1,"name":"Band","picture_xl":"https://e-cdns-images.dzcdn.net/images/artist/abc/1000x1000-000000-80-0-0.jpg"}]}"#;
    const PLACEHOLDER: &str = r#"{"data":[{"id":2,"name":"Band","picture_xl":"https://e-cdns-images.dzcdn.net/images/artist//1000x1000-000000-80-0-0.jpg"}]}"#;

    fn tmp() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("rp-portrait-mod-{}", fastrand::u64(..)))
    }

    #[test]
    fn found_downloads_and_stores_image() {
        let dir = tmp();
        let mut search = |_: &str| Ok(HIT.to_string());
        let mut download = |_: &str| Ok(png_bytes());
        let outcome =
            load_or_fetch_with("Band", false, 1_000, &dir, &mut search, &mut download).unwrap();
        match outcome {
            PortraitOutcome::Found(path) => assert!(path.exists()),
            PortraitOutcome::NotFound => panic!("expected Found"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn placeholder_is_notfound_and_writes_marker_without_download() {
        let dir = tmp();
        let mut search = |_: &str| Ok(PLACEHOLDER.to_string());
        let mut download = |_: &str| -> Result<Vec<u8>, FetchError> { panic!("must not download") };
        let outcome =
            load_or_fetch_with("Band", false, 1_000, &dir, &mut search, &mut download).unwrap();
        assert!(matches!(outcome, PortraitOutcome::NotFound));
        assert!(cache::negative_marker_path(&dir, "Band").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn fresh_cached_image_short_circuits_without_fetch() {
        let dir = tmp();
        cache::store_image(&dir, "Band", b"img", "jpg").unwrap();
        let now = cache::file_epoch_secs(&cache::portrait_path_in(&dir, "Band").unwrap());
        let mut search = |_: &str| -> Result<String, FetchError> { panic!("must not search") };
        let mut download = |_: &str| -> Result<Vec<u8>, FetchError> { panic!("must not download") };
        let outcome =
            load_or_fetch_with("Band", false, now, &dir, &mut search, &mut download).unwrap();
        assert!(matches!(outcome, PortraitOutcome::Found(_)));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn fresh_negative_marker_short_circuits() {
        let dir = tmp();
        cache::write_negative(&dir, "Band");
        let now = cache::file_epoch_secs(&cache::negative_marker_path(&dir, "Band"));
        let mut search = |_: &str| -> Result<String, FetchError> { panic!("must not search") };
        let mut download = |_: &str| -> Result<Vec<u8>, FetchError> { panic!("must not download") };
        let outcome =
            load_or_fetch_with("Band", false, now, &dir, &mut search, &mut download).unwrap();
        assert!(matches!(outcome, PortraitOutcome::NotFound));
        std::fs::remove_dir_all(&dir).ok();
    }
}
