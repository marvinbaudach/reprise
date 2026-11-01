//! The ordering policy of the album-cover triage (NET-7e): what settles an album without a
//! network request, cheapest check first. The one step that is not a stat of the cover cache,
//! resolving local art (a tag read over SAF on the phone), is injected, so the platform
//! adapter stays a thin closure and this module keeps no platform knowledge.

use std::path::Path;

use super::CoverAlbumTriage;
use crate::cover_download::{self, CoverState};

/// Stat-only stage: a cached cover, a fresh recorded miss or a fresh local-art marker settle
/// the album with no tag read. `None` means the album still has to be looked at.
#[must_use]
pub fn stat_only_triage_in(
    cache_root: &Path,
    album_artist: &str,
    album: &str,
) -> Option<CoverAlbumTriage> {
    match cover_download::album_cover_state_in(cache_root, album_artist, album) {
        CoverState::Cached(_) => Some(CoverAlbumTriage::Cached),
        CoverState::KnownMissing => Some(CoverAlbumTriage::KnownMissing),
        CoverState::Unknown => {
            cover_download::local_marker_is_fresh_in(cache_root, album_artist, album)
                .then_some(CoverAlbumTriage::LocalMarker)
        }
    }
}

/// Classifies one album: the stat-only stage first, then `resolves_local_art`. Local art
/// found there is recorded as a `.local` marker, so the next triage settles it by stat.
pub fn triage_album_in(
    cache_root: &Path,
    album_artist: &str,
    album: &str,
    resolves_local_art: impl FnOnce() -> bool,
) -> CoverAlbumTriage {
    if let Some(settled) = stat_only_triage_in(cache_root, album_artist, album) {
        return settled;
    }
    if !resolves_local_art() {
        return CoverAlbumTriage::NeedsRequest;
    }
    cover_download::write_local_marker_in(cache_root, album_artist, album);
    CoverAlbumTriage::LocalResolved
}

#[cfg(test)]
#[path = "cover_triage_tests.rs"]
mod tests;
