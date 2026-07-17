//! Keeps the unified filter bar's compact result count aligned with the
//! exact `TrackListModel` query without expanding the already large track
//! list composition module.

use std::cell::RefCell;
use std::rc::Rc;

use reprise_core::queries::{self, BrowseFilter};
use reprise_core::view_source::ViewSource;
use rusqlite::Connection;

use super::browse_bar::BrowseBar;

pub(in crate::ui) fn update(
    bar: &Rc<BrowseBar>,
    conn: &Rc<RefCell<Connection>>,
    source: &ViewSource,
    count: usize,
    search: &str,
    browse: &BrowseFilter,
    queue_ids: &[i64],
) {
    bar.set_source_context(source);
    bar.set_search(search);
    if !super::filter_restriction::is_track_source(source) {
        bar.hide_result_count();
        return;
    }
    let restricted = super::filter_restriction::is_restricted(search, browse);
    let total = {
        let conn = conn.borrow();
        source_total(&conn, source, restricted, count, queue_ids)
    };
    match total {
        Ok(total) => bar.set_result_count(count, total),
        Err(error) => {
            tracing::warn!(%error, "could not load total count for filter row");
            bar.hide_result_count();
        }
    }
}

fn source_total(
    conn: &Connection,
    source: &ViewSource,
    restricted: bool,
    count: usize,
    queue_ids: &[i64],
) -> Result<usize, rusqlite::Error> {
    if !restricted {
        return Ok(count);
    }
    queries::query_track_count_browsed(conn, source, "", &BrowseFilter::default(), queue_ids)
        .and_then(|value| {
            usize::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(0, value))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::view_source::ViewSource;

    fn seeded_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tracks (id,path,title,artist,album,added_at) VALUES
               (1,'/a.flac','Falling Apart','Caskets','X',0),
               (2,'/b.flac','Other','Dead by April','Y',0),
               (3,'/c.flac','Third','Z','Z',0);
             INSERT INTO playlists (id,name,position) VALUES (7,'P',0);
             INSERT INTO playlist_tracks (playlist_id,track_id,position) VALUES
               (7,1,1),(7,2,2);",
        )
        .unwrap();
        conn
    }

    // UX FIL-2: the total pairs the filtered count with the SOURCE's own
    // unfiltered size — a playlist restricted to 1 hit reads "1 of 2".
    #[test]
    fn fil_2_source_total_is_the_unfiltered_source_count() {
        let conn = seeded_conn();
        assert_eq!(
            source_total(&conn, &ViewSource::Playlist(7), true, 1, &[]).unwrap(),
            2
        );
        assert_eq!(
            source_total(&conn, &ViewSource::Library, true, 1, &[]).unwrap(),
            3
        );
    }

    // UX FIL-2: without restriction total == count (no second query).
    #[test]
    fn fil_2_source_total_equals_count_when_idle() {
        let conn = seeded_conn();
        assert_eq!(
            source_total(&conn, &ViewSource::Playlist(7), false, 2, &[]).unwrap(),
            2
        );
    }

    // UX FIL-2: the queue total needs the live queue ids.
    #[test]
    fn fil_2_queue_total_counts_the_queue_snapshot() {
        let conn = seeded_conn();
        assert_eq!(
            source_total(&conn, &ViewSource::Queue, true, 1, &[1, 2, 3]).unwrap(),
            3
        );
    }
}
