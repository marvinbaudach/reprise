//! `C1` — the remote image module: YouTube channel `thumbnails`, iTunes
//! `artworkUrl600`, and radio-browser `favicon`. Unblocks the image parts of
//! A2 and B1, which ship with the plain `source_image` glyph fallback until
//! this module exists (see `docs/plans/podcasts-youtube-radio-turn6.md`,
//! Block C).
//!
//! This module owns the pure caching/fetch policy only — no gtk4,
//! libadwaita, gstreamer, or zbus dependency, and no network client of its
//! own. Decoding bytes into a displayable texture, and the actual HTTP call,
//! stay in the GNOME crate and the caller respectively; [`resolve`] takes an
//! injected fetch closure so its tests never touch the network.
//!
//! `NET-1a`: the caller supplies `allowed`, already computed as
//! `online_sources::network_allowed(conn, &modules::SOURCE_IMAGES_MODULE)` —
//! this module never reads settings itself, mirroring `NET-3a`'s
//! injected-state style rather than looking things up internally. A cache
//! hit is returned regardless of `allowed`: NET-1a promises that turning a
//! module off never hides an already-cached image, only the fallback path on
//! a genuine cache miss is gated. A failed gate lookup upstream must already
//! have been treated as "not allowed" by the caller — this module has no
//! opinion on that, it only trusts the boolean it is given.

pub mod cache;

use std::path::PathBuf;

/// What [`resolve`] decided. `NotAllowed`, `NoUrl`, and `FetchFailed` all
/// mean the same thing to a caller: show the source glyph, never an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageOutcome {
    /// Already on disk — shown regardless of `allowed`.
    Cached(PathBuf),
    /// Freshly fetched and cached this call.
    Fetched(PathBuf),
    /// No cache entry, and the gate was closed — the fetch closure was never
    /// called.
    NotAllowed,
    /// No URL was supplied at all.
    NoUrl,
    /// The gate was open but the fetch, or the resulting bytes, failed
    /// (transport error, non-image response, oversized body, ...).
    FetchFailed,
}

/// Resolves a source image: cache hit first (no gate check — an
/// already-cached image is never hidden), then, only on a genuine miss, the
/// gate before any network work. `fetch` is called at most once and only
/// when `allowed` is true and there is no cache entry.
pub fn resolve(
    url: Option<&str>,
    allowed: bool,
    fetch: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
) -> ImageOutcome {
    let Some(url) = url.map(str::trim).filter(|url| !url.is_empty()) else {
        return ImageOutcome::NoUrl;
    };
    let dir = cache::cache_dir();
    if let Some(path) = cache::cached_path_in(&dir, url) {
        return ImageOutcome::Cached(path);
    }
    if !allowed {
        return ImageOutcome::NotAllowed;
    }
    match fetch(url) {
        Ok(bytes) => match crate::cover_download::validated_image_extension(&bytes) {
            Some(ext) => cache::store_image(&dir, url, &bytes, ext)
                .map_or(ImageOutcome::FetchFailed, ImageOutcome::Fetched),
            None => ImageOutcome::FetchFailed,
        },
        Err(_) => ImageOutcome::FetchFailed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_bytes() -> Vec<u8> {
        let mut buffer = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(1, 1)
            .write_to(&mut buffer, image::ImageFormat::Png)
            .unwrap();
        buffer.into_inner()
    }

    #[test]
    fn src_11_no_url_never_calls_fetch() {
        let mut fetch = |_: &str| -> Result<Vec<u8>, String> { panic!("must not fetch") };
        assert_eq!(resolve(None, true, &mut fetch), ImageOutcome::NoUrl);
        assert_eq!(resolve(Some(""), true, &mut fetch), ImageOutcome::NoUrl);
        assert_eq!(resolve(Some("   "), true, &mut fetch), ImageOutcome::NoUrl);
    }

    /// `SRC-11` / `NET-1a`: the core proof that the gate is checked before
    /// any network work — a fetch closure that panics if called must never
    /// fire when `allowed` is false and nothing is cached yet.
    #[test]
    fn src_11_gate_closed_never_calls_fetch_on_a_cache_miss() {
        let url = "https://images.test/net-1a-closed.jpg";
        let mut fetch = |_: &str| -> Result<Vec<u8>, String> { panic!("must not fetch") };
        assert_eq!(
            resolve(Some(url), false, &mut fetch),
            ImageOutcome::NotAllowed
        );
    }

    #[test]
    fn src_11_cache_hit_is_returned_without_checking_allowed() {
        let url = "https://images.test/net-1a-cached.jpg";
        let dir = cache::cache_dir();
        let path = cache::store_image(&dir, url, &png_bytes(), "png").unwrap();

        let mut fetch = |_: &str| -> Result<Vec<u8>, String> { panic!("must not fetch") };
        // `allowed: false` — NET-1a: an already-cached image is never hidden.
        assert_eq!(
            resolve(Some(url), false, &mut fetch),
            ImageOutcome::Cached(path)
        );
        std::fs::remove_file(cache::cached_path_in(&dir, url).unwrap()).ok();
    }

    #[test]
    fn src_11_allowed_and_uncached_fetches_validates_and_caches() {
        let url = "https://images.test/net-1a-fetch.jpg";
        let bytes = png_bytes();
        let mut fetch = |requested: &str| {
            assert_eq!(requested, url);
            Ok(bytes.clone())
        };
        let outcome = resolve(Some(url), true, &mut fetch);
        let ImageOutcome::Fetched(path) = outcome else {
            panic!("expected Fetched, got {outcome:?}");
        };
        assert!(path.exists());

        // A second resolve must now hit the cache and never fetch again.
        let mut must_not_fetch = |_: &str| -> Result<Vec<u8>, String> { panic!("must not fetch") };
        assert_eq!(
            resolve(Some(url), true, &mut must_not_fetch),
            ImageOutcome::Cached(path.clone())
        );
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn src_11_transport_error_yields_fetch_failed_without_caching() {
        let url = "https://images.test/net-1a-error.jpg";
        let mut fetch = |_: &str| -> Result<Vec<u8>, String> { Err("transport".into()) };
        assert_eq!(
            resolve(Some(url), true, &mut fetch),
            ImageOutcome::FetchFailed
        );
        assert!(cache::cached_path_in(&cache::cache_dir(), url).is_none());
    }

    #[test]
    fn src_11_non_image_bytes_yield_fetch_failed_without_caching() {
        let url = "https://images.test/net-1a-not-an-image.jpg";
        let mut fetch = |_: &str| -> Result<Vec<u8>, String> { Ok(b"not an image".to_vec()) };
        assert_eq!(
            resolve(Some(url), true, &mut fetch),
            ImageOutcome::FetchFailed
        );
        assert!(cache::cached_path_in(&cache::cache_dir(), url).is_none());
    }
}
