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
/// The advance shape from the live bug: one leading row removed, every
/// section boundary behind it shifted. `items-changed` covers no surviving
/// row, so GTK would keep its stale header tiles — the swap MUST also emit
/// `sections-changed` over the whole model.
#[test]
fn leading_removal_with_shifted_sections_also_emits_sections_changed() {
    assert_eq!(
        super::queue_snapshot_emissions((0, 1, 0), true, 5),
        (Some((0, 1, 0)), Some((0, 5)))
    );
}

#[test]
fn queue_snapshot_emissions_skips_redundant_and_illegal_signals() {
    // A full-range items-changed re-matches every header by itself.
    assert_eq!(
        super::queue_snapshot_emissions((0, 6, 5), true, 5),
        (Some((0, 6, 5)), None)
    );
    // Unchanged section ranges (plain context advance): items-changed only.
    assert_eq!(
        super::queue_snapshot_emissions((3, 1, 0), false, 5),
        (Some((3, 1, 0)), None)
    );
    // Sections moved without any row delta: sections-changed alone, no
    // fake full replace that would rebuild every row widget.
    assert_eq!(
        super::queue_snapshot_emissions((0, 0, 0), true, 5),
        (None, Some((0, 5)))
    );
    // Emptied queue: `gtk_section_model_sections_changed` requires
    // `n_items > 0`, so nothing may be emitted for a zero-row model.
    assert_eq!(
        super::queue_snapshot_emissions((0, 4, 0), true, 0),
        (Some((0, 4, 0)), None)
    );
}

use super::*;

fn track_items(ids: &[i64]) -> Vec<reprise_core::up_next::QueueItem> {
    ids.iter()
        .copied()
        .map(reprise_core::up_next::QueueItem::Track)
        .collect()
}

fn seeded_model(rows: &[(&str, &str)]) -> TrackListModel {
    let conn = crate::test_db::open().unwrap();
    for (t, a) in rows {
        crate::test_db::connection(&conn)
            .execute(
                "INSERT INTO tracks (path, title, artist, added_at) VALUES (?1, ?2, ?3, 0)",
                rusqlite::params![format!("/x/{t}.flac"), t, a],
            )
            .unwrap();
    }
    TrackListModel::new(Rc::new(conn))
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
    let conn = crate::test_db::open().unwrap();
    {
        let fixture_conn = crate::test_db::connection(&conn);
        let tx = fixture_conn.unchecked_transaction().unwrap();
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
    TrackListModel::new(Rc::new(conn))
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

    let queue = super::super::queue_sections::compose(None, &track_items(&[3, 1]), &[], None);
    model.set_queue_snapshot(&queue, vec![(0, 2)]);

    assert_eq!(model.n_items(), 2);
    assert!(model.cached_windows().is_empty());
    assert_eq!(model.track_at(0).unwrap().id, 3);
    assert_eq!(model.track_at(1).unwrap().id, 1);
    assert_eq!(model.cached_windows(), vec![0]);
}

#[test]
fn mixed_queue_snapshot_renders_track_and_episode_with_colliding_ids() {
    let conn = crate::test_db::open().unwrap();
    crate::test_db::connection(&conn)
        .execute_batch(
            "INSERT INTO tracks (id, path, title, artist, added_at)
             VALUES (7, '/x/track.flac', 'Library Seven', 'Track Artist', 0);
             INSERT INTO podcast_subscriptions
             (id, kind, feed_url, title, added_at)
             VALUES (1, 'rss', 'https://example.test/feed', 'Systems Weekly', 0);
             INSERT INTO podcast_episodes
             (id, subscription_id, guid, title, audio_url, duration_secs, first_seen_at)
             VALUES
             (7, 1, 'episode-seven', 'Episode Seven',
              'https://example.test/seven.mp3', 90, 0);",
        )
        .unwrap();
    let model = TrackListModel::new(Rc::new(conn));
    let queue = super::super::queue_sections::compose(
        None,
        &[
            reprise_core::up_next::QueueItem::Track(7),
            reprise_core::up_next::QueueItem::Episode(7),
        ],
        &[],
        None,
    );

    model.set_queue_snapshot(&queue, vec![(0, 2)]);

    assert!(matches!(
        model.queue_item_at(0),
        Some(reprise_core::queries::QueueItemMetadata::Track(track))
            if track.title == "Library Seven"
    ));
    assert!(matches!(
        model.queue_item_at(1),
        Some(reprise_core::queries::QueueItemMetadata::Episode(episode))
            if episode.title == "Episode Seven" && episode.show == "Systems Weekly"
    ));
    assert!(model.track_at(1).is_none());
}

#[test]
fn advancing_the_queue_emits_one_leading_removal_instead_of_a_full_replace() {
    let model = seeded_model(&[("One", "A"), ("Two", "B"), ("Three", "C")]);
    let live_tail = Rc::new(RefCell::new(vec![1, 2, 3]));
    let tail_for_before = live_tail.clone();
    let before = super::super::queue_sections::compose_virtual(
        None,
        &[],
        Some(
            super::super::queue_sections::VirtualContextTail::identified(
                3,
                (7, 11),
                1,
                Rc::new(move |offset, limit| {
                    tail_for_before
                        .borrow()
                        .iter()
                        .skip(offset)
                        .take(limit)
                        .copied()
                        .collect()
                }),
            ),
        ),
        None,
    );
    model.set_queue_snapshot(&before, vec![(0, 3)]);

    let changes = Rc::new(RefCell::new(Vec::new()));
    let changes_for_signal = changes.clone();
    model.connect_items_changed(move |_, position, removed, added| {
        changes_for_signal
            .borrow_mut()
            .push((position, removed, added));
    });

    *live_tail.borrow_mut() = vec![2, 3];
    let tail_for_after = live_tail.clone();
    let after = super::super::queue_sections::compose_virtual(
        None,
        &[],
        Some(
            super::super::queue_sections::VirtualContextTail::identified(
                2,
                (7, 11),
                2,
                Rc::new(move |offset, limit| {
                    tail_for_after
                        .borrow()
                        .iter()
                        .skip(offset)
                        .take(limit)
                        .copied()
                        .collect()
                }),
            ),
        ),
        None,
    );
    model.set_queue_snapshot(&after, vec![(0, 2)]);

    assert_eq!(
        *changes.borrow(),
        vec![(0, 1, 0)],
        "automatic advance must preserve the unchanged queue rows"
    );
}

#[test]
fn consuming_play_next_preserves_the_remaining_sidebar_rows() {
    let model = seeded_model(&[("One", "A"), ("Two", "B"), ("Three", "C")]);
    let before = super::super::queue_sections::compose(None, &track_items(&[1, 2, 3]), &[], None);
    model.set_queue_snapshot(&before, vec![(0, 3)]);

    let changes = Rc::new(RefCell::new(Vec::new()));
    let changes_for_signal = changes.clone();
    model.connect_items_changed(move |_, position, removed, added| {
        changes_for_signal
            .borrow_mut()
            .push((position, removed, added));
    });

    let after = super::super::queue_sections::compose(None, &track_items(&[2, 3]), &[], None);
    model.set_queue_snapshot(&after, vec![(0, 2)]);

    assert_eq!(
        *changes.borrow(),
        vec![(0, 1, 0)],
        "consuming Play Next must preserve the remaining sidebar rows"
    );
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
    let conn = crate::test_db::open().unwrap();
    for (t, missing_since) in [("Alpha", None), ("Beta", Some(1))] {
        crate::test_db::connection(&conn)
            .execute(
                "INSERT INTO tracks (path, title, artist, added_at, missing_since) \
                     VALUES (?1, ?2, '', 0, ?3)",
                rusqlite::params![format!("/x/{t}.flac"), t, missing_since],
            )
            .unwrap();
    }
    let model = TrackListModel::new(Rc::new(conn));

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
        let conn = &conn;
        let fixture_conn = crate::test_db::connection(conn.as_ref());
        let mut stmt = fixture_conn
            .prepare("SELECT id FROM tracks ORDER BY title")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    // ids sorted by title are [Alpha, Mid, Zulu]; reverse them so the
    // Queue order is the opposite of any column sort.
    let queue_items: Vec<_> = ids
        .into_iter()
        .rev()
        .map(reprise_core::up_next::QueueItem::Track)
        .collect();

    model.set_query(&ViewSource::Queue, "ignored", "ignored", "", &queue_items);
    assert_eq!(model.n_items(), 3);
    assert_eq!(model.track_at(0).unwrap().title, "Zulu");
    assert_eq!(model.track_at(1).unwrap().title, "Mid");
    assert_eq!(model.track_at(2).unwrap().title, "Alpha");
}
