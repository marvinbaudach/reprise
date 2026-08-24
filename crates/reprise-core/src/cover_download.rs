//! Automatic online album-cover download. Resolves a MusicBrainz release
//! and fetches its Cover Art Archive front cover into the `covers/downloaded/`
//! cache when the local cover pipeline has no usable image. Album downloads
//! are also published best-effort into the album's local track directories;
//! release-group covers remain cache-only because they have no local album.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::{
    cover, musicbrainz,
    source_error::{SourceError, SourceErrorKind},
};

pub(crate) const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp", "ico"];

const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

/// How long a release-group negative cover marker blocks a network re-fetch.
/// Unlike the album path (permanent negative cache), release-group covers can
/// appear on the Cover Art Archive after the fact, so a stale 404 gets
/// rechecked instead of being cached forever.
const NEGATIVE_MARKER_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Minimum MusicBrainz search score to even consider a release.
const MIN_MB_SCORE: i64 = 90;

impl From<&musicbrainz::FetchError> for SourceErrorKind {
    fn from(error: &musicbrainz::FetchError) -> Self {
        match error {
            musicbrainz::FetchError::Timeout
            | musicbrainz::FetchError::Transport
            | musicbrainz::FetchError::HttpStatus(_)
            | musicbrainz::FetchError::Body
            | musicbrainz::FetchError::BodyTooLarge => Self::Unreachable,
        }
    }
}

impl From<musicbrainz::FetchError> for SourceError {
    fn from(error: musicbrainz::FetchError) -> Self {
        let kind = SourceErrorKind::from(&error);
        Self::new(kind, "album cover request failed", error.to_string())
    }
}

enum CaaFetchResult {
    Found(Vec<u8>, &'static str),
    NotFound,
    TransientFailure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseGroupCover {
    Image(PathBuf),
    Fallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverState {
    Cached(PathBuf),
    KnownMissing,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverFetchOutcome {
    Downloaded(PathBuf),
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

pub(crate) fn downloaded_dir_in(cache_root: &Path) -> PathBuf {
    cover::cache_dir_with_root(cache_root).join("downloaded")
}

/// The cached downloaded cover file for `key`, if one exists (any known ext).
pub fn downloaded_cover_path(key: &str) -> Option<PathBuf> {
    let dir = downloaded_dir();
    downloaded_cover_path_from_dir(&dir, key)
}

pub(crate) fn downloaded_cover_path_in(cache_root: &Path, key: &str) -> Option<PathBuf> {
    downloaded_cover_path_from_dir(&downloaded_dir_in(cache_root), key)
}

fn downloaded_cover_path_from_dir(dir: &Path, key: &str) -> Option<PathBuf> {
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

pub fn caa_release_group_front_url(mbid: &str) -> String {
    format!(
        "https://coverartarchive.org/release-group/{}/front-250",
        musicbrainz::urlencode(mbid)
    )
}

fn release_group_key(mbid: &str) -> String {
    cover::hash_hex(format!("release-group\u{1}{}", mbid.trim()).as_bytes())
}

/// Does an existing release-group negative marker still block a network
/// re-fetch? Only while it's fresh (younger than `NEGATIVE_MARKER_MAX_AGE`).
/// A marker with no known mtime doesn't block; a marker whose mtime is in
/// the future (clock skew) is treated as fresh.
fn negative_marker_blocks(marker_modified: Option<SystemTime>, now: SystemTime) -> bool {
    match marker_modified {
        Some(modified) => now
            .duration_since(modified)
            .map_or(true, |age| age < NEGATIVE_MARKER_MAX_AGE),
        None => false,
    }
}

pub fn release_group_cover_path(mbid: &str) -> Option<PathBuf> {
    downloaded_cover_path(&release_group_key(mbid))
}

pub fn release_group_cover_state(mbid: &str) -> CoverState {
    release_group_cover_state_at(mbid, SystemTime::now())
}

fn release_group_cover_state_at(mbid: &str, now: SystemTime) -> CoverState {
    let key = release_group_key(mbid);
    let cached = downloaded_cover_path(&key);
    let marker_modified = std::fs::metadata(negative_marker_path(&key))
        .and_then(|metadata| metadata.modified())
        .ok();
    release_group_cover_state_from(cached, marker_modified, now)
}

fn release_group_cover_state_from(
    cached: Option<PathBuf>,
    marker_modified: Option<SystemTime>,
    now: SystemTime,
) -> CoverState {
    if let Some(path) = cached {
        return CoverState::Cached(path);
    }
    if negative_marker_blocks(marker_modified, now) {
        CoverState::KnownMissing
    } else {
        CoverState::Unknown
    }
}

pub fn fetch_release_group_cover(mbid: &str) -> ReleaseGroupCover {
    fetch_release_group_cover_with(mbid, &mut |url| http_get_bytes(url))
}

fn fetch_release_group_cover_with<F>(mbid: &str, fetch: &mut F) -> ReleaseGroupCover
where
    F: FnMut(&str) -> CaaFetchResult,
{
    let key = release_group_key(mbid);
    match release_group_cover_state(mbid) {
        CoverState::Cached(path) => return ReleaseGroupCover::Image(path),
        CoverState::KnownMissing => return ReleaseGroupCover::Fallback,
        CoverState::Unknown => {}
    }
    match fetch(&caa_release_group_front_url(mbid)) {
        CaaFetchResult::Found(bytes, extension) => store_downloaded(&key, &bytes, extension)
            .map_or(ReleaseGroupCover::Fallback, ReleaseGroupCover::Image),
        CaaFetchResult::NotFound => {
            write_negative(&key);
            ReleaseGroupCover::Fallback
        }
        CaaFetchResult::TransientFailure => ReleaseGroupCover::Fallback,
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ReleaseSearchResult {
    Match(String),
    NoMatch,
    Malformed,
}

fn parse_best_release(json: &str, album_artist: &str, album: &str) -> ReleaseSearchResult {
    fn norm(s: &str) -> String {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return ReleaseSearchResult::Malformed;
    };
    let Some(releases) = value.get("releases").and_then(serde_json::Value::as_array) else {
        return ReleaseSearchResult::Malformed;
    };
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
            let Some(id) = r.get("id").and_then(serde_json::Value::as_str) else {
                return ReleaseSearchResult::Malformed;
            };
            return ReleaseSearchResult::Match(id.to_owned());
        }
    }
    ReleaseSearchResult::NoMatch
}

pub fn fetch_and_cache(
    album_artist: &str,
    album: &str,
    mbid: Option<&str>,
    album_dirs: &[PathBuf],
) -> CoverFetchOutcome {
    fetch_and_cache_with(
        album_artist,
        album,
        mbid,
        album_dirs,
        &mut mb_get,
        &mut http_get_bytes,
    )
}

fn fetch_and_cache_with<M, C>(
    album_artist: &str,
    album: &str,
    mbid: Option<&str>,
    album_dirs: &[PathBuf],
    mb_fetch: &mut M,
    caa_fetch: &mut C,
) -> CoverFetchOutcome
where
    M: FnMut(&str) -> Option<String>,
    C: FnMut(&str) -> CaaFetchResult,
{
    let key = album_key(album_artist, album);
    // 1. Already resolved (positive or negative) -> no network.
    if let Some(existing) = downloaded_cover_path(&key) {
        return CoverFetchOutcome::Downloaded(existing);
    }
    if negative_marker_path(&key).exists() {
        return CoverFetchOutcome::NotFound;
    }
    // 2. Resolve a release MBID: embedded first, else conservative search.
    let release_mbid = match mbid {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => {
            let Some(body) = mb_fetch(&musicbrainz_search_url(album_artist, album)) else {
                return CoverFetchOutcome::TransientFailure;
            };
            match parse_best_release(&body, album_artist, album) {
                ReleaseSearchResult::Match(id) => id,
                ReleaseSearchResult::NoMatch => {
                    write_negative(&key);
                    return CoverFetchOutcome::NotFound;
                }
                ReleaseSearchResult::Malformed => return CoverFetchOutcome::TransientFailure,
            }
        }
    };
    // 3. Fetch the CAA front cover (follows the 302 to the image).
    let (bytes, ext) = match caa_fetch(&caa_front_url(&release_mbid)) {
        CaaFetchResult::Found(bytes, ext) => (bytes, ext),
        CaaFetchResult::NotFound => {
            write_negative(&key);
            return CoverFetchOutcome::NotFound;
        }
        CaaFetchResult::TransientFailure => return CoverFetchOutcome::TransientFailure,
    };
    // 4. Publish atomically under the download cache, then best-effort beside
    // the album tracks. Folder writeback never changes download success.
    store_album_downloaded(&key, &bytes, ext, album_dirs).map_or(
        CoverFetchOutcome::TransientFailure,
        CoverFetchOutcome::Downloaded,
    )
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
    classify_caa_body(bytes)
}

fn classify_caa_body(bytes: Vec<u8>) -> CaaFetchResult {
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return CaaFetchResult::TransientFailure;
    }
    match validated_image_extension(&bytes) {
        Some(ext) => CaaFetchResult::Found(bytes, ext),
        None => CaaFetchResult::TransientFailure,
    }
}

fn is_clean_caa_miss(status: u16) -> bool {
    status == 404
}

pub(crate) struct DecodedImage {
    image: image::DynamicImage,
    extension: Option<&'static str>,
}

impl DecodedImage {
    pub(crate) fn image(&self) -> &image::DynamicImage {
        &self.image
    }

    pub(crate) fn validated_extension(&self) -> Option<&'static str> {
        self.extension
    }
}

pub(crate) fn decode_image(bytes: &[u8]) -> Option<DecodedImage> {
    let format = image::guess_format(bytes).ok()?;
    let extension = match format {
        image::ImageFormat::Jpeg => Some("jpg"),
        image::ImageFormat::Png => Some("png"),
        image::ImageFormat::WebP => Some("webp"),
        image::ImageFormat::Gif => Some("gif"),
        image::ImageFormat::Bmp => Some("bmp"),
        image::ImageFormat::Ico => Some("ico"),
        _ => None,
    };
    Some(DecodedImage {
        image: image::load_from_memory_with_format(bytes, format).ok()?,
        extension,
    })
}

pub(crate) fn validated_image_extension(bytes: &[u8]) -> Option<&'static str> {
    decode_image(bytes)?.validated_extension()
}

fn write_negative(key: &str) {
    let _ = std::fs::create_dir_all(downloaded_dir());
    let _ = std::fs::write(negative_marker_path(key), b"");
}

/// Bumped whenever a downloaded cover is published.
///
/// A downloaded cover outranks a track's own artwork, so publishing one can
/// change what *any* track resolves to — including tracks that have not been
/// touched themselves. `cover::thumbnail_for_track` carries this marker's
/// timestamp in its stamp so those remembered answers fall. A marker rather
/// than the directory's own mtime, so that anything else writing into the
/// download cache does not invalidate every remembered cover in the library.
pub fn publish_marker() -> PathBuf {
    downloaded_dir().join(".published")
}

fn note_publication() {
    let marker = publish_marker();
    if let Some(dir) = marker.parent() {
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
    }
    let _ = std::fs::write(&marker, fastrand::u64(..).to_string());
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
    note_publication();
    Some(out)
}

fn store_album_downloaded(
    key: &str,
    bytes: &[u8],
    ext: &str,
    album_dirs: &[PathBuf],
) -> Option<PathBuf> {
    store_album_downloaded_with(
        key,
        bytes,
        ext,
        album_dirs,
        crate::cover_writeback::write_album_cover,
    )
}

fn store_album_downloaded_with(
    key: &str,
    bytes: &[u8],
    ext: &str,
    album_dirs: &[PathBuf],
    writeback: impl FnOnce(&[PathBuf], &[u8], &str) -> Vec<crate::cover_writeback::CoverWrite>,
) -> Option<PathBuf> {
    let cached = store_downloaded(key, bytes, ext)?;
    let _ = writeback(album_dirs, bytes, ext);
    Some(cached)
}

#[cfg(test)]
#[path = "cover_download_retry_tests.rs"]
mod retry_tests;

#[cfg(test)]
#[path = "cover_download_tests.rs"]
mod tests;
