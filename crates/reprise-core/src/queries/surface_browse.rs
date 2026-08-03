//! Narrow read façades for browse surfaces that do not expose the GTK track
//! list's column-sort, facet, paging, queue, or AI-projection controls.

use crate::db::Db;
use crate::models::Track;
use crate::up_next::QueueItem;
use crate::view_source::ViewSource;

use super::{query_album_canonical_track_ids, query_track_window, MAX_WINDOW_LIMIT};

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
