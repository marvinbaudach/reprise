//! Generated-metadata scalability probe for the lazy track-list model.

use std::time::Instant;

use super::*;

const DEFAULT_TRACK_COUNT: u32 = 10_000;
pub(super) const MAX_TRACK_COUNT: u32 = 100_000;
const SCROLL_SAMPLES: u32 = 12;
const MAX_CACHED_WINDOW_BUDGET: usize = 8;
// An absolute row ceiling, deliberately not derived from `MAX_CACHED_WINDOWS *
// WINDOW_SIZE`: derived, it would only restate the window assertion above and
// could never fail. Stated, it forces every window-size change to be a
// conscious memory decision.
//
// It was 1,600 while the lazy window held 200 rows. #647 took the window to 500
// to halve the browser's self-triggered reload, which raises the same eight
// windows to 4,000 rows. That trade — 2.5x the cached rows for ~40% off the
// reload — is the one recorded in
// docs/plans/search-reload-blocks-the-main-thread.md, Task 6.
const MAX_CACHED_TRACK_BUDGET: usize = 4_000;

fn configured_track_count() -> u32 {
    let count = std::env::var("REPRISE_PERF_TRACKS").map_or(DEFAULT_TRACK_COUNT, |value| {
        value
            .parse()
            .expect("REPRISE_PERF_TRACKS must be an integer")
    });
    assert!(
        (WINDOW_SIZE * SCROLL_SAMPLES..=MAX_TRACK_COUNT).contains(&count),
        "REPRISE_PERF_TRACKS must be between {} and {MAX_TRACK_COUNT}",
        WINDOW_SIZE * SCROLL_SAMPLES
    );
    count
}

fn title(index: u32) -> String {
    format!("track-{index:06}")
}

fn generated_model(count: u32) -> TrackListModel {
    let conn = crate::test_db::open().unwrap();
    {
        let fixture_conn = crate::test_db::connection(&conn);
        let tx = fixture_conn.unchecked_transaction().unwrap();
        {
            let mut insert = tx
                .prepare_cached(
                    "INSERT INTO tracks (path, title, artist, album, added_at) \
                     VALUES (?1, ?2, ?3, ?4, 0)",
                )
                .unwrap();
            for index in 0..count {
                insert
                    .execute(rusqlite::params![
                        format!("/synthetic/library/{index:06}.flac"),
                        title(index),
                        format!("Artist {:04}", index % 1_000),
                        format!("Album {:05}", index % 10_000),
                    ])
                    .unwrap();
            }
        }
        tx.commit().unwrap();
    }
    TrackListModel::new(Rc::new(conn))
}

#[test]
#[ignore = "performance baseline; run via scripts/performance-baseline.sh"]
fn generated_library_scroll_keeps_track_cache_bounded() {
    let track_count = configured_track_count();
    let model = generated_model(track_count);
    let started = Instant::now();

    model.set_query(&ViewSource::Library, "title", "asc", "", &[]);
    assert_eq!(model.n_items(), track_count);

    for sample in 0..SCROLL_SAMPLES {
        let position = (track_count - 1) * sample / (SCROLL_SAMPLES - 1);
        assert_eq!(model.track_at(position).unwrap().title, title(position));
    }

    let (cached_windows, cached_tracks) = model.cache_usage();
    assert!(
        cached_windows <= MAX_CACHED_WINDOW_BUDGET,
        "cached {cached_windows} windows, budget is {MAX_CACHED_WINDOW_BUDGET}"
    );
    assert!(
        cached_tracks <= MAX_CACHED_TRACK_BUDGET,
        "cached {cached_tracks} rows, budget is {MAX_CACHED_TRACK_BUDGET}"
    );
    eprintln!(
        "PERFORMANCE track_list tracks={track_count} elapsed_us={} cached_windows={cached_windows} cached_tracks={cached_tracks}",
        started.elapsed().as_micros()
    );
}
