//! M3U and PLS stream resolution.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaylistKind {
    M3u,
    Pls,
}

/// Extracts the first HTTP stream from one playlist document.
///
/// An HLS manifest is deliberately not resolved: GStreamer needs the manifest
/// URL, rather than one of its media-segment paths. Callers detect the `None`
/// result together with [`is_hls_manifest`] and keep the original URL.
pub fn resolve_playlist(body: &str, kind: PlaylistKind) -> Option<String> {
    if is_hls_manifest(body) {
        return None;
    }
    let candidate = match kind {
        PlaylistKind::Pls => body.lines().find_map(|line| {
            let (key, value) = line.split_once('=')?;
            let key = key.trim().to_ascii_lowercase();
            let index = key.strip_prefix("file")?;
            (!index.is_empty() && index.chars().all(|character| character.is_ascii_digit()))
                .then(|| value.trim())
                .filter(|value| is_http_url(value))
        }),
        PlaylistKind::M3u => body
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with('#')),
    }?;
    is_http_url(candidate).then(|| candidate.to_owned())
}

#[must_use]
pub fn is_hls_manifest(body: &str) -> bool {
    body.lines()
        .map(str::trim)
        .any(|line| line.starts_with("#EXT-X-"))
}

fn is_http_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rad_4_playlist_resolution_supports_pls_m3u_and_hls() {
        assert_eq!(
            resolve_playlist(
                "[playlist]\nNumberOfEntries=1\nFile1=https://radio.example/live\n",
                PlaylistKind::Pls,
            ),
            Some("https://radio.example/live".into())
        );
        assert_eq!(
            resolve_playlist(
                "#EXTM3U\n#EXTINF:-1,Example\nhttps://radio.example/live\n",
                PlaylistKind::M3u,
            ),
            Some("https://radio.example/live".into())
        );
        assert_eq!(
            resolve_playlist("#EXTM3U\n#EXT-X-VERSION:3\nsegment.ts\n", PlaylistKind::M3u),
            None,
            "HLS manifests must stay at their original URL"
        );
    }

    #[test]
    fn playlist_resolution_ignores_unsafe_or_empty_entries() {
        assert_eq!(
            resolve_playlist("[playlist]\nFile1=file:///etc/passwd\n", PlaylistKind::Pls),
            None
        );
        assert_eq!(
            resolve_playlist("# comments only\n", PlaylistKind::M3u),
            None
        );
    }

    #[test]
    fn pls_skips_metadata_keys_that_begin_with_file() {
        assert_eq!(
            resolve_playlist(
                "[playlist]\nFileVersion=2\nFile1=https://radio.example/live\n",
                PlaylistKind::Pls,
            ),
            Some("https://radio.example/live".into())
        );
    }
}
