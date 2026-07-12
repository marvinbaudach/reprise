//! Opt-in online album-cover download (GUI-A2). Resolves a MusicBrainz release
//! and fetches its Cover Art Archive front cover into the `covers/downloaded/`
//! cache. GATED: nothing here touches the network unless the `cover_download`
//! module is enabled (default off). Writes ONLY under the XDG cover cache.

use std::path::PathBuf;

use crate::cover;

const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp"];

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
