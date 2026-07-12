//! Opt-in online album-cover download (GUI-A2). Resolves a MusicBrainz release
//! and fetches its Cover Art Archive front cover into the `covers/downloaded/`
//! cache. GATED: nothing here touches the network unless the `cover_download`
//! module is enabled (default off). Writes ONLY under the XDG cover cache.

use std::path::PathBuf;

use crate::cover;

const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp"];

/// Minimum MusicBrainz search score to even consider a release.
const MIN_MB_SCORE: i64 = 90;

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

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "used by the GUI-A2 Task 3 fetch pipeline")
)]
pub(crate) fn musicbrainz_search_url(album_artist: &str, album: &str) -> String {
    // MusicBrainz Lucene query; percent-encode the whole query value.
    let query = format!("artist:\"{album_artist}\" AND release:\"{album}\"");
    format!(
        "https://musicbrainz.org/ws/2/release?query={}&fmt=json&limit=5",
        urlencode(&query)
    )
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "used by the GUI-A2 Task 3 fetch pipeline")
)]
pub(crate) fn caa_front_url(mbid: &str) -> String {
    format!("https://coverartarchive.org/release/{mbid}/front")
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "used by the GUI-A2 Task 3 fetch pipeline")
)]
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

/// Minimal percent-encoding for a query value (RFC 3986 unreserved kept).
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
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
}
