//! Deezer release-cover detection for artist portraits.

use std::collections::HashSet;

/// Deezer documents pagination for artist releases. The high bound keeps the
/// comparison to one request while covering even unusually large catalogues.
pub(crate) const ALBUM_PAGE_LIMIT: usize = 1_000;

pub(crate) fn albums_url(artist_id: u64) -> String {
    format!("https://api.deezer.com/artist/{artist_id}/albums?limit={ALBUM_PAGE_LIMIT}")
}

/// Returns `None` when Deezer did not provide a release collection, including
/// its HTTP-200 error objects. An empty `data` array is a successful comparison.
pub(crate) fn parse_release_cover_identifiers(json: &str) -> Option<HashSet<String>> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value.get("next").is_some_and(|next| !next.is_null()) {
        return None;
    }
    let releases = value.get("data")?.as_array()?;
    Some(
        releases
            .iter()
            .filter_map(|release| release.get("md5_image"))
            .filter_map(serde_json::Value::as_str)
            .filter(|identifier| !identifier.is_empty())
            .map(str::to_owned)
            .collect(),
    )
}

pub(crate) fn is_release_cover(picture_url: &str, covers: &HashSet<String>) -> bool {
    super::deezer::image_identifier(picture_url)
        .is_some_and(|identifier| covers.contains(&identifier))
}
