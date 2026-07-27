//! Announcement-URL selection for New Releases (NR-11, `[geplant]`).
//!
//! MusicBrainz release-groups are fetched with `inc=url-rels` (see
//! `artist_news::release_groups_url`), so each group's JSON carries a
//! `relations` array of `{"type": ..., "url": {"resource": ...}}` entries.
//! `parse_announce_url` picks the single best link to hand the user when
//! they ask to open the announcement; `announce_url_or_fallback` covers the
//! case where nothing usable was found (or nothing was ever persisted) by
//! pointing at the MusicBrainz release-group page instead.

use serde_json::Value;
use url::Url;

const BANDCAMP_HOST: &str = "bandcamp.com";
const PURCHASE_OR_STREAM_TYPES: &[&str] = &[
    "purchase for download",
    "free streaming",
    "download for free",
    "streaming",
];
const HOMEPAGE_TYPES: &[&str] = &["official homepage", "discography entry"];

/// Priorisierte Ankündigungs-URL aus den url-rels einer Release-Group.
/// Reihenfolge: purchase-for-download / free-streaming (Bandcamp-Domains
/// zuerst) → official homepage / discography entry → None.
pub fn parse_announce_url(group: &Value) -> Option<String> {
    let relations = group.get("relations")?.as_array()?;
    let links = relations
        .iter()
        .filter_map(|relation| {
            let kind = relation.get("type")?.as_str()?;
            let url = relation.get("url")?.get("resource")?.as_str()?;
            Some((kind, url))
        })
        .collect::<Vec<_>>();

    find_link(&links, PURCHASE_OR_STREAM_TYPES, true)
        .or_else(|| find_link(&links, HOMEPAGE_TYPES, false))
        .map(str::to_owned)
}

/// Returns the first link whose type matches `wanted`, preferring a
/// Bandcamp resource when `prefer_bandcamp` is set.
fn find_link<'a>(
    links: &[(&'a str, &'a str)],
    wanted: &[&str],
    prefer_bandcamp: bool,
) -> Option<&'a str> {
    let mut first = None;
    for &(kind, url) in links {
        if !wanted.contains(&kind) {
            continue;
        }
        if prefer_bandcamp && bandcamp_purchase_url(Some(url)).is_some() {
            return Some(url);
        }
        if first.is_none() {
            first = Some(url);
        }
    }
    first
}

/// Returns an unchanged launchable Bandcamp URL, rejecting lookalike hosts.
#[must_use]
pub fn bandcamp_purchase_url(value: Option<&str>) -> Option<&str> {
    let value = value?;
    let parsed = Url::parse(value).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let host = parsed.host_str()?;
    let mut segments = parsed.path_segments()?;
    let is_album_page = segments.next() == Some("album")
        && segments.next().is_some_and(|slug| !slug.trim().is_empty());
    ((host == BANDCAMP_HOST || host.ends_with(".bandcamp.com")) && is_album_page).then_some(value)
}

/// Persistierte URL oder Fallback auf die MB-Release-Group-Seite.
pub fn announce_url_or_fallback(stored: Option<&str>, release_group_mbid: &str) -> String {
    stored
        .filter(|url| crate::external_link::is_launchable_url(url))
        .map_or_else(
            || format!("https://musicbrainz.org/release-group/{release_group_mbid}"),
            str::to_owned,
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn nr_11_parse_announce_url_prefers_bandcamp_then_homepage() {
        let group = json!({
            "relations": [
                {"type": "official homepage", "url": {"resource": "https://band.example/"}},
                {"type": "free streaming", "url": {"resource": "https://open.spotify.com/album/x"}},
                {"type": "purchase for download", "url": {"resource": "https://band.bandcamp.com/album/x"}},
            ]
        });
        assert_eq!(
            parse_announce_url(&group),
            Some("https://band.bandcamp.com/album/x".to_string())
        );
    }

    #[test]
    fn nr_11_parse_announce_url_falls_back_to_homepage_without_purchase_links() {
        let group = json!({
            "relations": [
                {"type": "discography entry", "url": {"resource": "https://discogs.example/release/1"}},
                {"type": "official homepage", "url": {"resource": "https://band.example/"}},
            ]
        });
        assert_eq!(
            parse_announce_url(&group),
            Some("https://discogs.example/release/1".to_string())
        );
    }

    #[test]
    fn nr_11_parse_announce_url_is_none_without_matching_relations() {
        let group = json!({
            "relations": [
                {"type": "wikidata", "url": {"resource": "https://wikidata.example/Q1"}},
            ]
        });
        assert_eq!(parse_announce_url(&group), None);

        assert_eq!(parse_announce_url(&json!({})), None);
        assert_eq!(parse_announce_url(&json!({"relations": []})), None);
        assert_eq!(
            parse_announce_url(&json!({"relations": [{"type": "streaming"}]})),
            None
        );
    }

    #[test]
    fn nr_11_announce_url_or_fallback_prefers_stored_url() {
        assert_eq!(
            announce_url_or_fallback(None, "abc"),
            "https://musicbrainz.org/release-group/abc"
        );
        assert_eq!(
            announce_url_or_fallback(Some("https://artist.example/release"), "abc"),
            "https://artist.example/release"
        );
    }

    #[test]
    fn nr_11_announce_url_or_fallback_rejects_non_web_stored_url() {
        for stored in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "reprise://play/1",
            "",
        ] {
            assert_eq!(
                announce_url_or_fallback(Some(stored), "abc"),
                "https://musicbrainz.org/release-group/abc"
            );
        }
    }

    #[test]
    fn nr_20_bandcamp_purchase_url_accepts_only_real_bandcamp_hosts() {
        for candidate in [
            "https://oceansleeper.bandcamp.com/album/maybe-death-is-all-i-need",
            "https://bandcamp.com/album/example",
        ] {
            assert_eq!(bandcamp_purchase_url(Some(candidate)), Some(candidate));
        }

        for candidate in [
            "https://bandcamp.com.evil.example/album/fake",
            "https://evilbandcamp.com/album/fake",
            "https://oceansleeper.bandcamp.com/",
            "https://bandcamp.com/search?q=Ocean%20Sleeper",
            "file://bandcamp.com/etc/passwd",
            "javascript:bandcamp.com",
            "",
        ] {
            assert_eq!(bandcamp_purchase_url(Some(candidate)), None);
        }
        assert_eq!(bandcamp_purchase_url(None), None);
    }
}
