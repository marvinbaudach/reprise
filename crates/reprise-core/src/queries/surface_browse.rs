//! Counted, bounded read façades for browse surfaces. Scope, search, order,
//! and window are shared; GTK-specific facets, queues, AI projection, window
//! alignment, caching, and prefetch remain with their existing owners.

use crate::browser::SortDirection;
use crate::db::Db;
use crate::models::Track;
use crate::view_source::ViewSource;

use super::{query_track_count, query_track_window};

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
    /// Canonical disc/track order for an album container.
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

pub(super) fn has_more(total: i64, window: WindowRange, returned: usize) -> bool {
    let returned = i64::try_from(returned).unwrap_or(i64::MAX);
    window.offset.max(0).saturating_add(returned) < total
}

/// Runs the shared counted and bounded track query.
pub fn query_library_tracks(
    db: &Db,
    request: &LibraryTrackRequest,
) -> Result<TrackWindow, rusqlite::Error> {
    let source = match &request.scope {
        LibraryTrackScope::All => ViewSource::Library,
        LibraryTrackScope::Album {
            album,
            album_artist,
        } => ViewSource::Album {
            album: album.clone(),
            album_artist: album_artist.clone(),
        },
        LibraryTrackScope::Artist { artist } => ViewSource::Artist(artist.clone()),
    };
    let (sort_field, sort_dir) = match &request.order {
        LibraryTrackOrder::Sorted { field, direction } => (
            field.as_str(),
            match direction {
                SortDirection::Ascending => "asc",
                SortDirection::Descending => "desc",
            },
        ),
        LibraryTrackOrder::CanonicalAlbum => ("album_canonical", "asc"),
    };
    let total = query_track_count(db, &source, &request.search, &[])?;
    let rows = query_track_window(
        db,
        &source,
        sort_field,
        sort_dir,
        &request.search,
        request.window.offset,
        request.window.limit,
        &[],
    )?;
    Ok(TrackWindow {
        total,
        has_more: has_more(total, request.window, rows.len()),
        rows,
    })
}

/// Returns one counted window of an album's present tracks in canonical
/// disc/track order.
pub fn query_album_tracks(
    db: &Db,
    album: &str,
    album_artist: &str,
    window: WindowRange,
) -> Result<TrackWindow, rusqlite::Error> {
    query_library_tracks(
        db,
        &LibraryTrackRequest {
            scope: LibraryTrackScope::Album {
                album: album.to_owned(),
                album_artist: album_artist.to_owned(),
            },
            search: String::new(),
            order: LibraryTrackOrder::CanonicalAlbum,
            window,
        },
    )
}

/// Searches the present flat library with the shared literal LIKE semantics
/// and returns a counted window of matches in title order.
pub fn query_library_text_search(
    db: &Db,
    text: &str,
    window: WindowRange,
) -> Result<TrackWindow, rusqlite::Error> {
    query_library_tracks(
        db,
        &LibraryTrackRequest {
            scope: LibraryTrackScope::All,
            search: text.to_owned(),
            order: LibraryTrackOrder::Sorted {
                field: "title".to_owned(),
                direction: SortDirection::Ascending,
            },
            window,
        },
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
    fn text_search_returns_exact_total_and_gapless_windows() {
        let db = Db::open_in_memory().unwrap();
        for id in 1..=501_i64 {
            db.conn()
                .execute(
                    "INSERT INTO tracks (id,path,title,artist,added_at) VALUES (?1,?2,?3,'Match',0)",
                    rusqlite::params![id, format!("/music/{id:03}.flac"), format!("Track {id:03}")],
                )
                .unwrap();
        }

        let first = query_library_text_search(
            &db,
            "Match",
            WindowRange {
                offset: 0,
                limit: 500,
            },
        )
        .unwrap();
        let second = query_library_text_search(
            &db,
            "Match",
            WindowRange {
                offset: 500,
                limit: 500,
            },
        )
        .unwrap();

        assert_eq!(first.total, 501);
        assert_eq!(first.rows.len(), 500);
        assert!(first.has_more);
        assert_eq!(second.total, 501);
        assert_eq!(second.rows.len(), 1);
        assert!(!second.has_more);

        let ids = first
            .rows
            .iter()
            .chain(&second.rows)
            .map(|track| track.id)
            .collect::<Vec<_>>();
        assert_eq!(ids, (1..=501_i64).collect::<Vec<_>>());
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

        let titles = query_album_tracks(
            &db,
            "Album",
            "Artist",
            WindowRange {
                offset: 0,
                limit: 200,
            },
        )
        .unwrap()
        .rows
        .into_iter()
        .map(|track| track.title)
        .collect::<Vec<_>>();

        assert_eq!(
            titles,
            ["Disc 1 Track 1", "Disc 1 Track 2", "Disc 2 Track 1"]
        );
    }
}
