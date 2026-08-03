//! Narrow read façades for browse surfaces that do not expose the GTK track
//! list's column-sort, facet, paging, queue, or AI-projection controls.

use crate::browser::SortDirection;
use crate::db::Db;
use crate::models::Track;
use crate::up_next::QueueItem;
use crate::view_source::ViewSource;

use super::{query_album_canonical_track_ids, query_track_window, MAX_WINDOW_LIMIT};

/// The library subset a surface wants to read through a bounded window.
///
/// These variants deliberately cover only the shared browse surfaces proven
/// by the Android spike. Playlist, queue, genre, and desktop-only sources keep
/// their existing, richer contracts until another surface needs them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryTrackScope {
    All,
    Album { album: String, album_artist: String },
    Artist { artist: String },
}

/// Ordering for a library track window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryTrackOrder {
    /// A surface-selected column order, resolved through Core's existing
    /// whitelist before it reaches SQL.
    Sorted {
        field: String,
        direction: SortDirection,
    },
    /// Canonical disc/track order for an album playback snapshot.
    CanonicalAlbum,
}

/// One arbitrary window requested by a surface.
///
/// Alignment, cache size, and prefetch policy stay outside Core. The signed
/// values match SQLite and the existing query seam; Core clamps invalid or
/// oversized limits rather than allowing an unbounded read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowRange {
    pub offset: i64,
    pub limit: i64,
}

/// Owned inputs for the cross-surface library-track query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryTrackRequest {
    pub scope: LibraryTrackScope,
    pub search: String,
    pub order: LibraryTrackOrder,
    pub window: WindowRange,
}

/// Exact result cardinality plus the requested track window.
///
/// `has_more` is explicit so a caller never has to infer truncation by
/// comparing the row count with `total` itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackWindow {
    pub total: i64,
    pub rows: Vec<Track>,
    pub has_more: bool,
}

/// Returns one album's present tracks in canonical disc/track order.
pub fn query_album_tracks(
    db: &Db,
    album: &str,
    album_artist: &str,
) -> Result<Vec<Track>, rusqlite::Error> {
    let queue = query_album_canonical_track_ids(db, album, album_artist)?
        .into_iter()
        .map(QueueItem::Track)
        .collect::<Vec<_>>();
    query_track_window(
        db,
        &ViewSource::Queue,
        "",
        "",
        "",
        0,
        MAX_WINDOW_LIMIT,
        &queue,
    )
}

/// Searches the present flat library with the shared literal LIKE semantics
/// and returns matches in title order.
pub fn query_library_text_search(db: &Db, text: &str) -> Result<Vec<Track>, rusqlite::Error> {
    query_track_window(
        db,
        &ViewSource::Library,
        "title",
        "asc",
        text,
        0,
        MAX_WINDOW_LIMIT,
        &[],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn library_window_contract_owns_inputs_and_reports_continuation() {
        assert_send_sync::<LibraryTrackRequest>();
        assert_send_sync::<TrackWindow>();

        let request = LibraryTrackRequest {
            scope: LibraryTrackScope::Album {
                album: "Album".to_owned(),
                album_artist: "Artist".to_owned(),
            },
            search: "live".to_owned(),
            order: LibraryTrackOrder::Sorted {
                field: "title".to_owned(),
                direction: crate::browser::SortDirection::Ascending,
            },
            window: WindowRange {
                offset: 200,
                limit: 200,
            },
        };
        let response = TrackWindow {
            total: 401,
            rows: Vec::new(),
            has_more: true,
        };

        assert_eq!(request.window.offset, 200);
        assert_eq!(response.total, 401);
        assert!(response.has_more);
    }

    #[test]
    fn album_tracks_use_canonical_disc_then_track_order() {
        let db = Db::open_in_memory().unwrap();
        db.conn()
            .execute_batch(
                "INSERT INTO tracks
                   (id,path,title,artist,album,album_artist,disc_no,track_no,added_at) VALUES
                 (1,'/music/disc-2-track-1.flac','Disc 2 Track 1','Artist','Album','Artist',2,1,0),
                 (2,'/music/disc-1-track-2.flac','Disc 1 Track 2','Artist','Album','Artist',1,2,0),
                 (3,'/music/disc-1-track-1.flac','Disc 1 Track 1','Artist','Album','Artist',1,1,0);",
            )
            .unwrap();

        let titles = query_album_tracks(&db, "Album", "Artist")
            .unwrap()
            .into_iter()
            .map(|track| track.title)
            .collect::<Vec<_>>();

        assert_eq!(
            titles,
            ["Disc 1 Track 1", "Disc 1 Track 2", "Disc 2 Track 1"]
        );
    }
}
