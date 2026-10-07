//! Read-only reads behind the Android media browse tree (Android Auto and the
//! other media-browser clients).
//!
//! Every method is a thin adapter over a query `reprise-core` already has: a
//! browse tree decides what a head unit may list, it never invents library
//! semantics. Playlists and recently played tracks are the two sources the
//! Albums/Artists reads in `browse` and `filtered_browse` do not cover.

use reprise_core::library::playlists;
use reprise_core::queries::{
    self, AiColumn, RowWindow, TrackSort, TrackViewQuery, MAX_WINDOW_LIMIT,
};
use reprise_core::view_source::ViewSource;

use crate::{LibraryError, MusicLibrary, TrackRow};

/// One manual playlist as the browse tree lists it.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PlaylistRow {
    pub id: i64,
    pub name: String,
    /// Every stored membership, including rows whose file is currently
    /// missing — the same count the desktop sidebar shows.
    pub track_count: i64,
}

fn query_error(error: impl std::fmt::Display) -> LibraryError {
    LibraryError::Query {
        detail: error.to_string(),
    }
}

impl MusicLibrary {
    /// Present tracks ordered by their last play, newest first. Tracks that
    /// were never played carry no `last_played_at` and are left out.
    fn recently_played_rows(
        &self,
        limit: i64,
    ) -> Result<Vec<reprise_core::models::Track>, LibraryError> {
        let limit = limit.clamp(0, MAX_WINDOW_LIMIT);
        let reader = self.reader()?;
        let source = ViewSource::Library;
        let rows = queries::query_track_window(
            &reader,
            &TrackViewQuery::new(&source),
            TrackSort {
                field: "last_played_at",
                dir: "desc",
            },
            RowWindow { offset: 0, limit },
            AiColumn::Skip,
        )
        .map_err(query_error)?;
        // SQLite sorts NULL last under DESC, so the played tracks form a
        // prefix; filtering rather than trusting that keeps the answer right
        // if the sort expression ever changes.
        Ok(rows
            .into_iter()
            .filter(|track| track.last_played_at.is_some())
            .collect())
    }
}

#[uniffi::export]
impl MusicLibrary {
    /// Lists the manual playlists in the user's own order.
    pub fn list_playlists(&self) -> Result<Vec<PlaylistRow>, LibraryError> {
        let reader = self.reader()?;
        playlists::list(&reader)
            .map(|rows| {
                rows.into_iter()
                    .map(|playlist| PlaylistRow {
                        id: playlist.id,
                        name: playlist.name,
                        track_count: playlist.track_count,
                    })
                    .collect()
            })
            .map_err(query_error)
    }

    /// Returns one playlist's present tracks, in playlist order, as the ids a
    /// play request queues. Missing files are left out: they cannot play.
    pub fn playlist_track_ids(&self, playlist_id: i64) -> Result<Vec<i64>, LibraryError> {
        let reader = self.reader()?;
        let source = ViewSource::Playlist(playlist_id);
        queries::query_track_ids(
            &reader,
            &TrackViewQuery::new(&source),
            // The playlist ids query always follows playlist order and
            // ignores this sort; the field only has to be a valid one.
            TrackSort {
                field: "playlist_order",
                dir: "asc",
            },
        )
        .map_err(query_error)
    }

    /// The same present tracks as [`Self::playlist_track_ids`], as rows, so a
    /// listing needs one call instead of one lookup per track.
    pub fn playlist_tracks(&self, playlist_id: i64) -> Result<Vec<TrackRow>, LibraryError> {
        let reader = self.reader()?;
        queries::query_playlist_tracks_full(&reader, playlist_id)
            .map_err(query_error)
            .and_then(|tracks| crate::track_segment::track_rows(&reader, tracks))
    }

    /// Returns the ids of the most recently played present tracks, newest
    /// first. `limit` is clamped to the window cap.
    pub fn recently_played_track_ids(&self, limit: i64) -> Result<Vec<i64>, LibraryError> {
        Ok(self
            .recently_played_rows(limit)?
            .into_iter()
            .map(|track| track.id)
            .collect())
    }

    /// The same tracks as [`Self::recently_played_track_ids`], as rows.
    pub fn recently_played_tracks(&self, limit: i64) -> Result<Vec<TrackRow>, LibraryError> {
        let tracks = self.recently_played_rows(limit)?;
        let reader = self.reader()?;
        crate::track_segment::track_rows(&reader, tracks)
    }

    /// Resolves the uri a player was handed back to its library row. `None`
    /// when no present track lives there — a stale uri is an ordinary answer.
    /// A file cut into tracks by a CUE sheet resolves to the first of its
    /// tracks, in play order, that is still present.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI hands owned strings across the FFI boundary"
    )]
    pub fn track_by_uri(&self, uri: String) -> Result<Option<TrackRow>, LibraryError> {
        let reader = self.reader()?;
        for id in queries::track_ids_for_path(&reader, &uri).map_err(query_error)? {
            if let Some(track) =
                queries::query_present_track_by_id(&reader, id).map_err(query_error)?
            {
                return crate::track_segment::track_row(&reader, track).map(Some);
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
#[path = "media_browse_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "media_browse_port_tests.rs"]
mod port_tests;
