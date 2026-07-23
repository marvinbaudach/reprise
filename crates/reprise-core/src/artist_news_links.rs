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

const BANDCAMP_MARKER: &str = "bandcamp.com";
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
        if prefer_bandcamp && url.contains(BANDCAMP_MARKER) {
            return Some(url);
        }
        if first.is_none() {
            first = Some(url);
        }
    }
    first
}

/// Persistierte URL oder Fallback auf die MB-Release-Group-Seite.
pub fn announce_url_or_fallback(stored: Option<&str>, release_group_mbid: &str) -> String {
    stored.map_or_else(
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
        assert_eq!(announce_url_or_fallback(Some("x"), "abc"), "x");
    }
}
