//! On-demand album-cover fetch, mirroring `artist_portrait_fetch`
//! (`../artist_portrait.rs`): gated by the same artwork consent, offline
//! resolution always wins over a request (decision 9), and a definitive miss
//! is remembered per process the way the desktop's cover-download worker
//! remembers one (`cover_download_worker.rs:220-236`).

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock, PoisonError};

use reprise_core::cover;
use reprise_core::cover_download::{self, CoverFetchOutcome};
use reprise_core::queries;

use crate::{AndroidArtworkSize, LibraryError, MusicLibrary};

type Attempted = HashMap<String, CoverFetchOutcome>;

fn attempted() -> &'static Mutex<Attempted> {
    static ATTEMPTED: OnceLock<Mutex<Attempted>> = OnceLock::new();
    ATTEMPTED.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The cover pass that rides the artist-portrait backfill (B3): one handle
/// for the process, the same rule as [`attempted`] and for the same reason
/// — `MusicLibrary` gains no new field, and the FFI's forwarding closure in
/// `start_artist_portrait_backfill` (`../artist_portrait.rs`) needs a handle
/// that outlives any one call.
pub(super) fn cover_backfill() -> &'static reprise_core::artist_portrait::CoverBackfill {
    static COVER_BACKFILL: OnceLock<reprise_core::artist_portrait::CoverBackfill> = OnceLock::new();
    COVER_BACKFILL.get_or_init(reprise_core::artist_portrait::CoverBackfill::new)
}

/// `cargo test` runs every test of the crate in one process, and this
/// module's state is a process-global `static`; every test touching this
/// module (and `artist_portrait_tests.rs`, which shares the same statics)
/// calls this first so one test's state cannot leak into the next.
#[cfg(test)]
pub(crate) fn reset_album_cover_state_for_tests() {
    *attempted().lock().unwrap_or_else(PoisonError::into_inner) = HashMap::new();
    cover_backfill().cancel();
}

#[uniffi::export]
impl MusicLibrary {
    /// Fetches an album's cover on demand: the now-playing rung and the
    /// album detail page both call this only when local resolution already
    /// came back empty (decision 9). `Ok(None)` covers every case that keeps
    /// showing the generated placeholder — the gate is off, the album is
    /// unknown, or the fetch found nothing — the same fold `track_artwork`
    /// already uses for "no artwork".
    pub fn album_cover_fetch(
        &self,
        track_uri: &str,
        size: AndroidArtworkSize,
    ) -> Result<Option<String>, LibraryError> {
        let cache_root = self.cache_root.clone();
        self.album_cover_fetch_with(track_uri, size, &move |album_artist, album, mbid| {
            cover_download::fetch_and_cache_in(&cache_root, album_artist, album, mbid, &[])
        })
    }
}

impl MusicLibrary {
    fn album_cover_fetch_with(
        &self,
        track_uri: &str,
        size: AndroidArtworkSize,
        fetch: &dyn Fn(&str, &str, Option<&str>) -> CoverFetchOutcome,
    ) -> Result<Option<String>, LibraryError> {
        let allowed = {
            let reader = self.reader()?;
            reprise_core::online_sources::network_allowed_or_off(
                &reader,
                &reprise_core::modules::ARTWORK_MODULE,
            )
        };
        if !allowed {
            return Ok(None);
        }

        let (_, source) = self.configured_tree()?;
        let (album_artist, album) = {
            let reader = self.reader()?;
            match queries::query_stats_album_target_for_path(&reader, track_uri) {
                Ok(Some((_, album, album_artist))) => (album_artist, album),
                Ok(None) => return Ok(None),
                Err(error) => {
                    return Err(LibraryError::Query {
                        detail: error.to_string(),
                    })
                }
            }
        };
        if album.trim().is_empty() || album_artist.trim().is_empty() {
            return Ok(None);
        }

        // Offline resolution first: real local art always wins, no request.
        if let Some(existing) = self.track_artwork(track_uri, size)? {
            return Ok(Some(existing));
        }

        let path = Path::new(track_uri);
        let mbid = cover::read_cover_tag_with_source(source.as_ref(), path).release_mbid;
        let key = cover_download::album_key(&album_artist, &album);
        let memorised = attempted()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&key)
            .cloned();
        if let Some(CoverFetchOutcome::NotFound) = memorised {
            return Ok(None);
        }

        let outcome = fetch(&album_artist, &album, mbid.as_deref());
        if !matches!(outcome, CoverFetchOutcome::TransientFailure) {
            attempted()
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(key, outcome.clone());
        }
        match outcome {
            CoverFetchOutcome::Downloaded(_) => self.track_artwork(track_uri, size),
            CoverFetchOutcome::NotFound | CoverFetchOutcome::TransientFailure => Ok(None),
        }
    }
}

#[cfg(test)]
#[path = "album_cover_tests.rs"]
mod tests;
