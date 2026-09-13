//! Keeps the unified filter bar's compact result count aligned with the
//! exact `TrackListModel` query without expanding the already large track
//! list composition module.

use std::rc::Rc;

use reprise_core::db::Db;
use reprise_core::queries::{self, BrowseFilter};
use reprise_core::view_source::ViewSource;

use super::browse_bar::BrowseBar;

const MILLIS_PER_HOUR: i64 = 60 * 60 * 1_000;
const HOURS_PER_DAY: i64 = 24;

fn idle_library_caption(count: usize, total_duration_ms: i64) -> String {
    let (count_text, _) = crate::ui::filter_bar_strings::result_count_markup(count, count);
    let total_hours = total_duration_ms.max(0) / MILLIS_PER_HOUR;
    format!(
        "{count_text} · {} d {} h",
        total_hours / HOURS_PER_DAY,
        total_hours % HOURS_PER_DAY
    )
}

#[allow(clippy::too_many_arguments)]
pub(in crate::ui) fn update(
    bar: &Rc<BrowseBar>,
    conn: &Rc<Db>,
    source: &ViewSource,
    count: usize,
    search: &str,
    browse: &BrowseFilter,
    exclude_ai: bool,
    queue_ids: &[i64],
) {
    bar.set_source_context(source);
    bar.set_search(search);
    if !super::filter_restriction::is_track_source(source) {
        bar.hide_result_count();
        return;
    }
    let restricted = super::filter_restriction::is_restricted(search, browse, exclude_ai);
    let total = source_total(conn, source, restricted, count, queue_ids);
    match total {
        Ok(total) if matches!(source, ViewSource::Library) && !restricted => {
            match queries::query_library_stats_browsed(conn, "", &BrowseFilter::default()) {
                Ok(stats) => bar.set_result_count_caption(
                    count,
                    total,
                    &idle_library_caption(count, stats.total_duration_ms),
                ),
                Err(error) => {
                    tracing::warn!(%error, "could not load library duration for filter row");
                    bar.set_result_count(count, total);
                }
            }
        }
        Ok(total) => bar.set_result_count(count, total),
        Err(error) => {
            tracing::warn!(%error, "could not load total count for filter row");
            bar.hide_result_count();
        }
    }
}

fn source_total(
    conn: &Db,
    source: &ViewSource,
    restricted: bool,
    count: usize,
    queue_ids: &[i64],
) -> Result<usize, rusqlite::Error> {
    if !restricted || matches!(source, ViewSource::Queue) {
        return Ok(count);
    }
    // The counting base is always the current place. Substituting the library
    // here is what made an artist page read "3 of 9 tracks" — filter vocabulary
    // at a location that is not a filter (FIL-2).
    let queue_items = queue_ids
        .iter()
        .copied()
        .map(reprise_core::up_next::QueueItem::Track)
        .collect::<Vec<_>>();
    queries::query_track_count_browsed(conn, source, "", &BrowseFilter::default(), &queue_items)
        .and_then(|value| {
            usize::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(0, value))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::view_source::ViewSource;

    fn seeded_conn() -> Db {
        let conn = crate::test_db::open().unwrap();
        crate::test_db::connection(&conn)
            .execute_batch(
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

    // UX FIL-2a: the total pairs the filtered count with the SOURCE's own
    // unfiltered size — a playlist restricted to 1 hit reads "1 of 2".
    #[test]
    fn fil_2a_source_total_is_the_unfiltered_source_count() {
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

    // UX FIL-2a: without restriction total == count (no second query).
    #[test]
    fn fil_2a_source_total_equals_count_when_idle() {
        let conn = seeded_conn();
        assert_eq!(
            source_total(&conn, &ViewSource::Playlist(7), false, 2, &[]).unwrap(),
            2
        );
    }

    // UX FIL-2a: inside a place the counter relates to that place, never to the
    // whole library — a playlist reporting its own length is the precedent.
    #[test]
    fn fil_2a_place_counts_against_itself_not_the_library() {
        let conn = seeded_conn();
        let artist = ViewSource::Artist("Caskets".into());

        assert_eq!(source_total(&conn, &artist, true, 1, &[]).unwrap(), 1);
        assert_eq!(
            source_total(&conn, &ViewSource::Library, true, 1, &[]).unwrap(),
            3,
            "the library still counts against itself"
        );
    }

    // The virtual QUE-7 context tail already supplies the authoritative
    // Queue row count; filtering does not materialize it into an id list.
    #[test]
    fn fil_2a_queue_total_counts_the_queue_snapshot() {
        let conn = seeded_conn();
        assert_eq!(
            source_total(&conn, &ViewSource::Queue, true, 3, &[]).unwrap(),
            3
        );
    }

    #[test]
    fn fil_10_idle_caption_carries_count_and_duration() {
        assert_eq!(
            idle_library_caption(1_881, ((4 * 24 + 6) * 60 + 28) * 60 * 1_000),
            "1,881 tracks · 4 d 6 h"
        );
        assert_eq!(idle_library_caption(1, 3_600_000), "1 track · 0 d 1 h");
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn fil_10_duration_failure_keeps_the_known_count() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let conn = Rc::new(seeded_conn());
        let bar = BrowseBar::new(conn.clone());
        crate::test_db::connection(&conn)
            .execute_batch("DROP TABLE tracks")
            .unwrap();

        update(
            &bar,
            &conn,
            &ViewSource::Library,
            3,
            "",
            &BrowseFilter::default(),
            false,
            &[],
        );

        assert_eq!(bar.result_count(), Some((3, 3)));
    }
}
