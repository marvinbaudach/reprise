//! Display names used by M3U playlist import and export.

use std::path::Path;

use reprise_core::models::Track;

use super::strings;

/// Derives the imported playlist name from the M3U file stem, falling back
/// to a translated generic name when the stem is missing or blank.
pub(super) fn playlist_name_from_file(file_path: &Path) -> String {
    let fallback = strings::text(strings::IMPORTED_PLAYLIST_FALLBACK_NAME);
    file_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::trim)
        .filter(|stem| !stem.is_empty())
        .unwrap_or(&fallback)
        .to_string()
}

/// Returns `"Artist - Title"`, or just the title when the artist is blank.
pub(super) fn display_name(track: &Track) -> String {
    if track.artist.trim().is_empty() {
        track.title.clone()
    } else {
        format!("{} - {}", track.artist, track.title)
    }
}
