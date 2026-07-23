#[test]
fn section_for_answers_ranges_and_full_model_fallback() {
    let ranges = [(0u32, 1u32), (1, 3), (3, 6)];
    assert_eq!(super::imp::section_for(&ranges, 6, 0), (0, 1));
    assert_eq!(super::imp::section_for(&ranges, 6, 2), (1, 3));
    assert_eq!(super::imp::section_for(&ranges, 6, 5), (3, 6));
    // Past the declared ranges (transient sections/total mismatch): the
    // answer must be a NON-overlapping tail section, never one starting
    // at 0 — an answer overlapping an already-matched header is exactly
    // what GTK's gtk_list_item_manager_ensure_items asserts on (seen
    // live: abort on switching to the Queue view from a deep-scrolled
    // larger view).
    assert_eq!(super::imp::section_for(&ranges, 6, 9), (6, 10));
    // The live crash shape: a 2-row queue's ranges against a still-500-
    // row model, viewport tracking position 499.
    assert_eq!(
        super::imp::section_for(&[(0, 1), (1, 2)], 500, 499),
        (2, 500)
    );
    // No sections declared: the whole model stays one section.
    assert_eq!(super::imp::section_for(&[], 42, 7), (0, 42));
}
use super::*;

fn seeded_model(rows: &[(&str, &str)]) -> TrackListModel {
    let conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    for (t, a) in rows {
        conn.execute(
            "INSERT INTO tracks (path, title, artist, added_at) VALUES (?1, ?2, ?3, 0)",
            rusqlite::params![format!("/x/{t}.flac"), t, a],
        )
        .unwrap();
    }
    TrackListModel::new(Rc::new(RefCell::new(conn)))
}

/// Sortable, zero-padded title for row `i` (e.g. `track-00042`), used by
/// the bulk-seeding tests below so the expected sort order is a trivial
/// function of the row index.
fn bulk_title(i: u32) -> String {
    format!("track-{i:05}")
}

/// Seeds `count` rows in a single transaction (fast even for thousands
/// of rows) with titles from `bulk_title`, so ascending title sort order
/// matches ascending insertion/index order.
fn seeded_model_bulk(count: u32) -> TrackListModel {
    let mut conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    {
        let tx = conn.transaction().unwrap();
        for i in 0..count {
            let title = bulk_title(i);
            tx.execute(
                "INSERT INTO tracks (path, title, artist, added_at) VALUES (?1, ?2, ?3, 0)",
                rusqlite::params![format!("/x/{i:05}.flac"), title, "Bulk Artist"],
            )
            .unwrap();
        }
        tx.commit().unwrap();
    }
    TrackListModel::new(Rc::new(RefCell::new(conn)))
}

#[test]
fn set_query_updates_n_items_from_count() {
    let model = seeded_model(&[("Zulu", "AAA"), ("Alpha", "BBB"), ("Mid", "CCC")]);
    assert_eq!(model.n_items(), 0);
    model.set_query(&ViewSource::Library, "title", "asc", "", &[]);
    assert_eq!(model.n_items(), 3);
}

#[test]
fn queue_snapshot_defers_metadata_until_a_row_is_requested() {
    let model = seeded_model(&[("One", "A"), ("Two", "B"), ("Three", "C")]);

    let queue = super::super::queue_sections::compose(None, &[3, 1], &[], None);
    model.set_queue_snapshot(&queue, vec![(0, 2)]);

    assert_eq!(model.n_items(), 2);
    assert!(model.cached_windows().is_empty());
    assert_eq!(model.track_at(0).unwrap().id, 3);
    assert_eq!(model.track_at(1).unwrap().id, 1);
    assert_eq!(model.cached_windows(), vec![0]);
}

#[test]
fn set_query_applies_filter_to_count_and_rows() {
    let model = seeded_model(&[("Zulu", "AAA"), ("Alpha", "BBB"), ("Mid", "CCC")]);
    model.set_query(&ViewSource::Library, "title", "asc", "zu", &[]);
    assert_eq!(model.n_items(), 1);
    assert_eq!(model.track_at(0).unwrap().title, "Zulu");
}

#[test]
fn track_at_loads_in_sorted_order() {
    let model = seeded_model(&[("Zulu", "AAA"), ("Alpha", "BBB"), ("Mid", "CCC")]);
    model.set_query(&ViewSource::Library, "title", "asc", "", &[]);
    assert_eq!(model.track_at(0).unwrap().title, "Alpha");
    assert_eq!(model.track_at(1).unwrap().title, "Mid");
    assert_eq!(model.track_at(2).unwrap().title, "Zulu");
}

#[test]
fn track_at_out_of_range_returns_none() {
    let model = seeded_model(&[("Alpha", "BBB")]);
    model.set_query(&ViewSource::Library, "title", "asc", "", &[]);
    assert!(model.track_at(5).is_none());
}

#[test]
fn set_query_clears_stale_cache_between_queries() {
    let model = seeded_model(&[("Zulu", "AAA"), ("Alpha", "BBB")]);
    model.set_query(&ViewSource::Library, "title", "asc", "", &[]);
    assert_eq!(model.track_at(0).unwrap().title, "Alpha");
    model.set_query(&ViewSource::Library, "title", "desc", "", &[]);
    assert_eq!(model.track_at(0).unwrap().title, "Zulu");
}

/// Regression test: all prior tests seeded <=3 rows, so every `track_at`
/// call stayed inside window 0 and never exercised the window-boundary
/// math (`window_start`/`offset_in_window`) for a second or later
/// window. Seed >200 rows (WINDOW_SIZE) so position 200 falls in window
/// 1 and position 449 falls in window 2, and check both land on the
/// title the sort order predicts.
#[test]
fn track_at_spans_multiple_windows_in_sorted_order() {
    const ROW_COUNT: u32 = 450;
    let model = seeded_model_bulk(ROW_COUNT);
    model.set_query(&ViewSource::Library, "title", "asc", "", &[]);
    assert_eq!(model.n_items(), ROW_COUNT);

    assert_eq!(model.track_at(0).unwrap().title, bulk_title(0));
    assert_eq!(model.track_at(200).unwrap().title, bulk_title(200));
    assert_eq!(model.track_at(449).unwrap().title, bulk_title(449));

    assert!(model.track_at(ROW_COUNT).is_none());
}

#[test]
fn invalidate_window_at_forces_a_fresh_read_of_that_row() {
    let model = seeded_model(&[("Zulu", "AAA"), ("Alpha", "BBB")]);
    model.set_query(&ViewSource::Library, "title", "asc", "", &[]);
    assert_eq!(model.track_at(0).unwrap().rating, 0);

    // Mutate the underlying row directly (simulating a rating write
    // elsewhere), bypassing the model entirely.
    {
        let conn = model.imp().conn.borrow().clone().unwrap();
        conn.borrow()
            .execute("UPDATE tracks SET rating = 4 WHERE title = 'Alpha'", [])
            .unwrap();
    }

    // Without invalidation the cached clone is still stale.
    assert_eq!(model.track_at(0).unwrap().rating, 0);

    model.invalidate_window_at(0);
    assert_eq!(model.track_at(0).unwrap().rating, 4);
}

#[test]
fn invalidate_window_at_out_of_range_is_a_no_op() {
    let model = seeded_model(&[("Alpha", "BBB")]);
    model.set_query(&ViewSource::Library, "title", "asc", "", &[]);
    // Must not panic.
    model.invalidate_window_at(5);
}

#[test]
fn set_cached_rating_patches_the_cache_without_a_model_signal() {
    let model = seeded_model(&[("Zulu", "AAA"), ("Alpha", "BBB")]);
    model.set_query(&ViewSource::Library, "title", "asc", "", &[]);
    // Prime the cache for the window covering rows 0..2.
    assert_eq!(model.track_at(0).unwrap().rating, 0);

    // `items_changed` is the fake remove+insert that snaps the ColumnView
    // viewport to the top on a rating click; the in-place path must emit
    // none.
    let signals = Rc::new(std::cell::Cell::new(0u32));
    let signals_for_cb = signals.clone();
    model.connect_items_changed(move |_, _, _, _| {
        signals_for_cb.set(signals_for_cb.get() + 1);
    });

    model.set_cached_rating(0, 5);

    // The cached clone the model hands back on a later scroll-away/back now
    // carries the new rating...
    assert_eq!(model.track_at(0).unwrap().rating, 5);
    // ...and no remove+insert was emitted.
    assert_eq!(signals.get(), 0);
}

#[test]
fn set_cached_rating_on_an_uncached_window_is_a_no_op() {
    let model = seeded_model(&[("Alpha", "BBB")]);
    model.set_query(&ViewSource::Library, "title", "asc", "", &[]);
    // Nothing cached for any window yet; must not panic, and the next read
    // pulls fresh from SQL (unchanged there in this test).
    model.set_cached_rating(0, 3);
    assert_eq!(model.track_at(0).unwrap().rating, 0);
}

/// Regression test: eviction (`MAX_CACHED_WINDOWS` = 8, drop the
/// lowest-indexed cached window) was never exercised because no test
/// touched more than one window. Seed 1700 rows (9 windows) and touch
/// one position per window in ascending order so the 9th touch forces
/// an eviction; assert the cache holds exactly 8 windows, window 0 was
/// evicted, and the just-loaded window 8 is present.
#[test]
fn track_at_evicts_lowest_window_past_cache_capacity() {
    const ROW_COUNT: u32 = 1700;
    const WINDOW_COUNT: u32 = 9;
    let model = seeded_model_bulk(ROW_COUNT);
    model.set_query(&ViewSource::Library, "title", "asc", "", &[]);

    for window in 0..WINDOW_COUNT {
        let position = window * WINDOW_SIZE;
        assert_eq!(
            model.track_at(position).unwrap().title,
            bulk_title(position)
        );
    }

    let mut cached = model.cached_windows();
    cached.sort_unstable();
    assert_eq!(cached.len(), MAX_CACHED_WINDOWS);
    assert_eq!(cached, vec![200, 400, 600, 800, 1000, 1200, 1400, 1600]);
    assert!(!cached.contains(&0), "window 0 should have been evicted");
    assert!(
        cached.contains(&(8 * WINDOW_SIZE)),
        "just-loaded window 8 should be present"
    );
}

/// Stage 3 Task 3: `set_query`/`track_at` must plumb a non-Library
/// `ViewSource` through to `queries::query_track_window`/`query_track_
/// count` unchanged — exercised here end-to-end through the model
/// (`queries.rs`'s own test module covers each source's SQL directly).
#[test]
fn set_query_with_missing_source_shows_only_missing_rows() {
    let conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    for (t, missing_since) in [("Alpha", None), ("Beta", Some(1))] {
        conn.execute(
            "INSERT INTO tracks (path, title, artist, added_at, missing_since) \
                 VALUES (?1, ?2, '', 0, ?3)",
            rusqlite::params![format!("/x/{t}.flac"), t, missing_since],
        )
        .unwrap();
    }
    let model = TrackListModel::new(Rc::new(RefCell::new(conn)));

    model.set_query(&ViewSource::Missing, "title", "asc", "", &[]);
    assert_eq!(model.n_items(), 1);
    assert_eq!(model.track_at(0).unwrap().title, "Beta");
}

/// `ViewSource::Queue` reads its rows from the `queue_ids` param, not
/// from any `WHERE` clause — this pins that `set_query`/`track_at`
/// actually thread that slice through to `queries::query_track_window`
/// and preserve its order.
#[test]
fn set_query_with_queue_source_follows_queue_ids_order() {
    let model = seeded_model(&[("Zulu", "AAA"), ("Alpha", "BBB"), ("Mid", "CCC")]);
    let ids: Vec<i64> = {
        let conn = model.imp().conn.borrow().clone().unwrap();
        let conn = conn.borrow();
        let mut stmt = conn
            .prepare("SELECT id FROM tracks ORDER BY title")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    // ids sorted by title are [Alpha, Mid, Zulu]; reverse them so the
    // Queue order is the opposite of any column sort.
    let queue_ids: Vec<i64> = ids.into_iter().rev().collect();

    model.set_query(&ViewSource::Queue, "ignored", "ignored", "", &queue_ids);
    assert_eq!(model.n_items(), 3);
    assert_eq!(model.track_at(0).unwrap().title, "Zulu");
    assert_eq!(model.track_at(1).unwrap().title, "Mid");
    assert_eq!(model.track_at(2).unwrap().title, "Alpha");
}
