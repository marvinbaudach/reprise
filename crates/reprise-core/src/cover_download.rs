//! Automatic online album-cover download. Resolves a MusicBrainz release
//! and fetches its Cover Art Archive front cover into the `covers/downloaded/`
//! cache when the local cover pipeline has no usable image. Writes ONLY under
//! the XDG cover cache.

use std::path::PathBuf;
use std::time::Duration;

use crate::{cover, musicbrainz};

const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp"];

const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

/// Minimum MusicBrainz search score to even consider a release.
const MIN_MB_SCORE: i64 = 90;

enum CaaFetchResult {
    Found(Vec<u8>, &'static str),
    NotFound,
    TransientFailure,
}

/// Cache key for an album's downloaded cover: normalized album-artist + album,
/// hashed to hex. One cover per album — every track of an album shares it.
pub fn album_key(album_artist: &str, album: &str) -> String {
    fn norm(s: &str) -> String {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }
    cover::hash_hex(format!("{}\u{1}{}", norm(album_artist), norm(album)).as_bytes())
}

/// `<XDG cache>/reprise/covers/downloaded`.
pub fn downloaded_dir() -> PathBuf {
    cover::cache_dir().join("downloaded")
}

/// The cached downloaded cover file for `key`, if one exists (any known ext).
pub fn downloaded_cover_path(key: &str) -> Option<PathBuf> {
    let dir = downloaded_dir();
    IMAGE_EXTS
        .iter()
        .map(|ext| dir.join(format!("{key}.{ext}")))
        .find(|p| p.exists())
}

/// Marker written when a lookup found nothing — stops re-querying that album.
pub fn negative_marker_path(key: &str) -> PathBuf {
    downloaded_dir().join(format!("{key}.notfound"))
}

pub(crate) fn musicbrainz_search_url(album_artist: &str, album: &str) -> String {
    // MusicBrainz Lucene query; percent-encode the whole query value.
    let query = format!("artist:\"{album_artist}\" AND release:\"{album}\"");
    format!(
        "https://musicbrainz.org/ws/2/release?query={}&fmt=json&limit=5",
        musicbrainz::urlencode(&query)
    )
}

pub(crate) fn caa_front_url(mbid: &str) -> String {
    format!("https://coverartarchive.org/release/{mbid}/front")
}

pub(crate) fn parse_best_release(json: &str, album_artist: &str, album: &str) -> Option<String> {
    fn norm(s: &str) -> String {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let releases = v.get("releases")?.as_array()?;
    let (want_artist, want_album) = (norm(album_artist), norm(album));
    for r in releases {
        let score = r
            .get("score")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        if score < MIN_MB_SCORE {
            continue;
        }
        let title = r
            .get("title")
            .and_then(|title| title.as_str())
            .unwrap_or_default();
        let artist = r
            .get("artist-credit")
            .and_then(|artists| artists.as_array())
            .and_then(|artists| artists.first())
            .and_then(|credit| credit.get("name"))
            .and_then(|name| name.as_str())
            .unwrap_or_default();
        if norm(title) == want_album && norm(artist) == want_artist {
            return r.get("id").and_then(|id| id.as_str()).map(str::to_string);
        }
    }
    None
}

pub fn fetch_and_cache(album_artist: &str, album: &str, mbid: Option<&str>) -> Option<PathBuf> {
    let key = album_key(album_artist, album);
    // 1. Already resolved (positive or negative) -> no network.
    if let Some(existing) = downloaded_cover_path(&key) {
        return Some(existing);
    }
    if negative_marker_path(&key).exists() {
        return None;
    }
    // 2. Resolve a release MBID: embedded first, else conservative search.
    let release_mbid = match mbid {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => {
            let body = mb_get(&musicbrainz_search_url(album_artist, album))?;
            match parse_best_release(&body, album_artist, album) {
                Some(id) => id,
                None => {
                    write_negative(&key);
                    return None;
                }
            }
        }
    };
    // 3. Fetch the CAA front cover (follows the 302 to the image).
    let (bytes, ext) = match http_get_bytes(&caa_front_url(&release_mbid)) {
        CaaFetchResult::Found(bytes, ext) => (bytes, ext),
        CaaFetchResult::NotFound => {
            write_negative(&key);
            return None;
        }
        CaaFetchResult::TransientFailure => return None,
    };
    // 4. Publish atomically under the download cache.
    store_downloaded(&key, &bytes, ext)
}

/// A rate-limited MusicBrainz GET returning the response body as text.
fn mb_get(url: &str) -> Option<String> {
    musicbrainz::get(url).ok()
}

/// A plain GET returning validated image bytes, a clean miss, or a retryable failure.
fn http_get_bytes(url: &str) -> CaaFetchResult {
    let user_agent = musicbrainz::user_agent();
    let response = match ureq::Agent::config_builder()
        .timeout_global(Some(HTTP_TIMEOUT))
        .user_agent(&user_agent)
        .build()
        .new_agent()
        .get(url)
        .call()
    {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(status)) if is_clean_caa_miss(status) => {
            return CaaFetchResult::NotFound;
        }
        Err(_) => return CaaFetchResult::TransientFailure,
    };
    let mut bytes = Vec::new();
    use std::io::Read;
    if response
        .into_body()
        .into_reader()
        .take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return CaaFetchResult::TransientFailure;
    }
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return CaaFetchResult::NotFound;
    }
    match validated_image_extension(&bytes) {
        Some(ext) => CaaFetchResult::Found(bytes, ext),
        None => CaaFetchResult::NotFound,
    }
}

fn is_clean_caa_miss(status: u16) -> bool {
    status == 404
}

fn validated_image_extension(bytes: &[u8]) -> Option<&'static str> {
    let format = image::guess_format(bytes).ok()?;
    let ext = match format {
        image::ImageFormat::Jpeg => "jpg",
        image::ImageFormat::Png => "png",
        image::ImageFormat::WebP => "webp",
        image::ImageFormat::Gif => "gif",
        image::ImageFormat::Bmp => "bmp",
        _ => return None,
    };
    image::load_from_memory_with_format(bytes, format).ok()?;
    Some(ext)
}

fn write_negative(key: &str) {
    let _ = std::fs::create_dir_all(downloaded_dir());
    let _ = std::fs::write(negative_marker_path(key), b"");
}

fn store_downloaded(key: &str, bytes: &[u8], ext: &str) -> Option<PathBuf> {
    let dir = downloaded_dir();
    std::fs::create_dir_all(&dir).ok()?;
    let out = dir.join(format!("{key}.{ext}"));
    let tmp = dir.join(format!(".{key}-{}.{ext}.tmp", fastrand::u64(..)));
    if std::fs::write(&tmp, bytes).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return None;
    }
    if std::fs::rename(&tmp, &out).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return downloaded_cover_path(key); // a concurrent writer may have published it
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB_STRONG: &str = r#"{"releases":[
      {"id":"11111111-1111-1111-1111-111111111111","score":100,
       "title":"The Wall","artist-credit":[{"name":"Pink Floyd"}]}]}"#;
    const MB_WEAK: &str = r#"{"releases":[
      {"id":"22222222-2222-2222-2222-222222222222","score":42,
       "title":"Something Else","artist-credit":[{"name":"Other Band"}]}]}"#;

    #[test]
    fn album_key_normalizes_case_and_whitespace() {
        assert_eq!(
            album_key("Pink Floyd", "The Wall"),
            album_key("  pink   floyd ", "the wall")
        );
    }

    #[test]
    fn album_key_distinguishes_different_albums() {
        assert_ne!(album_key("A", "X"), album_key("A", "Y"));
        assert_ne!(album_key("A", "X"), album_key("B", "X"));
    }

    #[test]
    fn downloaded_dir_is_under_cache_dir() {
        assert!(downloaded_dir().starts_with(crate::cover::cache_dir()));
    }

    #[test]
    fn downloaded_cover_path_finds_an_existing_file_and_none_otherwise() {
        let key = album_key("FetchTest", "OnlyHere");
        assert!(downloaded_cover_path(&key).is_none());
        std::fs::create_dir_all(downloaded_dir()).unwrap();
        let f = downloaded_dir().join(format!("{key}.jpg"));
        std::fs::write(&f, b"x").unwrap();
        assert_eq!(downloaded_cover_path(&key), Some(f.clone()));
        std::fs::remove_file(&f).ok();
    }

    #[test]
    fn parse_best_release_accepts_a_strong_match() {
        assert_eq!(
            parse_best_release(MB_STRONG, "Pink Floyd", "The Wall").as_deref(),
            Some("11111111-1111-1111-1111-111111111111")
        );
    }

    #[test]
    fn parse_best_release_rejects_a_weak_match() {
        assert!(parse_best_release(MB_WEAK, "Pink Floyd", "The Wall").is_none());
    }

    #[test]
    fn parse_best_release_handles_empty_and_garbage() {
        assert!(parse_best_release(r#"{"releases":[]}"#, "A", "B").is_none());
        assert!(parse_best_release("not json", "A", "B").is_none());
    }

    #[test]
    fn urls_are_well_formed() {
        assert!(musicbrainz_search_url("Pink Floyd", "The Wall")
            .starts_with("https://musicbrainz.org/ws/2/release"));
        assert_eq!(
            caa_front_url("11111111-1111-1111-1111-111111111111"),
            "https://coverartarchive.org/release/11111111-1111-1111-1111-111111111111/front"
        );
    }

    #[test]
    fn fetch_returns_cached_path_without_network_when_already_downloaded() {
        let key = album_key("CachedBand", "CachedAlbum");
        std::fs::create_dir_all(downloaded_dir()).unwrap();
        let f = downloaded_dir().join(format!("{key}.png"));
        std::fs::write(&f, b"img").unwrap();
        // Already cached -> must return it, never touching the network.
        assert_eq!(
            fetch_and_cache("CachedBand", "CachedAlbum", None),
            Some(f.clone())
        );
        std::fs::remove_file(&f).ok();
    }

    #[test]
    fn fetch_short_circuits_on_negative_marker_without_network() {
        let key = album_key("MissBand", "MissAlbum");
        std::fs::create_dir_all(downloaded_dir()).unwrap();
        let marker = negative_marker_path(&key);
        std::fs::write(&marker, b"").unwrap();
        assert_eq!(fetch_and_cache("MissBand", "MissAlbum", None), None);
        std::fs::remove_file(&marker).ok();
    }

    #[test]
    fn only_caa_not_found_is_a_clean_http_miss() {
        assert!(is_clean_caa_miss(404));
        assert!(!is_clean_caa_miss(500));
        assert!(!is_clean_caa_miss(429));
    }

    #[test]
    fn downloaded_bytes_must_decode_as_a_supported_image() {
        assert_eq!(validated_image_extension(b"not an image"), None);

        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(1, 1)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        assert_eq!(validated_image_extension(png.get_ref()), Some("png"));
    }
}
