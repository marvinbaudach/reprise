//! `ViewSource::Queue` test coverage — split out of the former single-file
//! `queries.rs`'s inline test module (Refactoring & Extensibility Task 1)
//! purely to keep every file under the project's 800-line rule; see
//! `tests.rs`'s doc comment for the full split map. A pure move, no
//! assertion change.

use super::*;

fn seeded_conn_with_tracks(count: i64) -> Connection {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    for i in 1..=count {
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) \
             VALUES (?1, ?2, ?3, '', 0)",
            rusqlite::params![i, format!("/x/{i}.flac"), format!("Track {i}")],
        )
        .unwrap();
    }
    conn
}

#[test]
fn que_6_metadata_loads_in_one_query() {
    let conn = seeded_conn_with_tracks(4);
    let queue_ids = vec![4, 2, 3, 1];
    let query_count = std::cell::Cell::new(0);

    let rows = super::queue::query_track_window_queue_counted(
        &conn,
        &queue_ids,
        0,
        queue_ids.len() as i64,
        || query_count.set(query_count.get() + 1),
    )
    .unwrap();

    assert_eq!(query_count.get(), 1);
    assert_eq!(
        rows.into_iter().map(|track| track.id).collect::<Vec<_>>(),
        queue_ids
    );
}

#[test]
fn queue_duration_sums_duplicates_and_skips_stale_ids_in_one_batch() {
    let conn = seeded_conn_with_tracks(2);
    conn.execute("UPDATE tracks SET duration_ms = id * 1000", [])
        .unwrap();

    assert_eq!(
        query_queue_duration_ms(&conn, &[2, 1, 2, 999]).unwrap(),
        5_000
    );
    assert_eq!(query_queue_duration_ms(&conn, &[]).unwrap(), 0);
}

#[test]
fn queue_window_follows_the_ids_order_not_id_order() {
    let conn = seeded_conn_with_tracks(3);
    let mut conn = conn;
    let queue_ids = vec![3, 1, 2];
    let rows = query_track_window(
        &mut conn,
        &ViewSource::Queue,
        "ignored",
        "ignored",
        "ignored",
        0,
        10,
        &queue_ids,
    )
    .unwrap();
    let ids: Vec<i64> = rows.iter().map(|t| t.id).collect();
    assert_eq!(ids, vec![3, 1, 2]);
}

#[test]
fn queue_window_skips_ids_with_no_matching_row() {
    let conn = seeded_conn_with_tracks(3);
    let mut conn = conn;
    let queue_ids = vec![3, 999, 1];
    let rows = query_track_window(
        &mut conn,
        &ViewSource::Queue,
        "ignored",
        "ignored",
        "",
        0,
        10,
        &queue_ids,
    )
    .unwrap();
    let ids: Vec<i64> = rows.iter().map(|t| t.id).collect();
    assert_eq!(ids, vec![3, 1]);
}

#[test]
fn queue_window_slices_by_offset_and_limit_then_reorders() {
    let conn = seeded_conn_with_tracks(5);
    let mut conn = conn;
    let queue_ids = vec![5, 4, 3, 2, 1];
    let rows = query_track_window(
        &mut conn,
        &ViewSource::Queue,
        "ignored",
        "ignored",
        "",
        2,
        2,
        &queue_ids,
    )
    .unwrap();
    let ids: Vec<i64> = rows.iter().map(|t| t.id).collect();
    assert_eq!(ids, vec![3, 2]);
}

#[test]
fn queue_count_counts_resolvable_ids_regardless_of_filter() {
    let conn = seeded_conn_with_tracks(3);
    let queue_ids = vec![3, 2, 1];
    assert_eq!(
        query_track_count(&conn, &ViewSource::Queue, "anything", &queue_ids).unwrap(),
        3
    );
}

/// Stage-3 close-out regression: a queued id that no longer resolves to
/// a `tracks` row (e.g. hard-deleted via "Remove from library") must not
/// inflate the count past what `query_track_window`'s `Queue` arm can
/// actually render.
#[test]
fn queue_count_excludes_ids_that_no_longer_resolve_to_a_row() {
    let conn = seeded_conn_with_tracks(3);
    let queue_ids = vec![3, 999, 1]; // 999 was never inserted
    assert_eq!(
        query_track_count(&conn, &ViewSource::Queue, "", &queue_ids).unwrap(),
        2
    );
}

#[test]
fn queue_count_counts_each_occurrence_of_a_duplicated_resolvable_id() {
    let conn = seeded_conn_with_tracks(3);
    let queue_ids = vec![1, 1, 2]; // id 1 queued twice
    assert_eq!(
        query_track_count(&conn, &ViewSource::Queue, "", &queue_ids).unwrap(),
        3
    );
}

#[test]
fn queue_count_is_zero_for_an_empty_queue() {
    let conn = seeded_conn_with_tracks(3);
    assert_eq!(
        query_track_count(&conn, &ViewSource::Queue, "", &[]).unwrap(),
        0
    );
}

#[test]
fn queue_ids_are_returned_verbatim() {
    let queue_ids = vec![5, 4, 3];
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    assert_eq!(
        query_track_ids(&conn, &ViewSource::Queue, "x", "x", "", &queue_ids).unwrap(),
        queue_ids
    );
}

/// Regression for the `Queue` count/window invariant: when every id in
/// `queue_ids` resolves to a live row, `query_track_count`'s `Queue` arm
/// must equal the actual number of rows a full-window `query_track_
/// window` call returns.
#[test]
fn queue_count_matches_window_row_count_when_all_ids_resolve() {
    let mut conn = seeded_conn_with_tracks(5);
    let queue_ids = vec![5, 4, 3, 2, 1];

    let count = query_track_count(&conn, &ViewSource::Queue, "", &queue_ids).unwrap();
    let rows = query_track_window(
        &mut conn,
        &ViewSource::Queue,
        "ignored",
        "ignored",
        "",
        0,
        queue_ids.len() as i64,
        &queue_ids,
    )
    .unwrap();

    assert_eq!(count as usize, rows.len());
    assert_eq!(count as usize, queue_ids.len());
}

/// Stage-3 close-out: the desync this fix closes. `query_track_window_
/// queue` already silently dropped any id with no matching row; before
/// this fix, `query_track_count`'s `Queue` arm trusted `queue_ids.len()`
/// verbatim, so a `ColumnView` could be told there were more rows than
/// it would ever render (`count=4` while the window renders 3). Both
/// must now agree, even with a stale (hard-deleted) id still present in
/// `queue_ids` — the case that's reachable now that hard-delete exists.
#[test]
fn queue_count_matches_window_row_count_when_some_ids_do_not_resolve() {
    let mut conn = seeded_conn_with_tracks(3);
    let queue_ids = vec![3, 999, 1, 2]; // 999 doesn't resolve

    let count = query_track_count(&conn, &ViewSource::Queue, "", &queue_ids).unwrap();
    let rows = query_track_window(
        &mut conn,
        &ViewSource::Queue,
        "ignored",
        "ignored",
        "",
        0,
        queue_ids.len() as i64,
        &queue_ids,
    )
    .unwrap();

    assert_eq!(count as usize, rows.len());
    assert_eq!(count, 3);
}

/// Stage-3 close-out, dup-id follow-up: the bug this fix closes.
/// `query_track_window_queue` used to resolve each window slot via a
/// `HashMap::remove`-based drain, so the *second* occurrence of a
/// duplicated queue id (e.g. a track added to the queue twice, or
/// select-all -> add) found nothing left in the map and was silently
/// dropped — the view rendered one row where the queue had two, and
/// since queue DnD-reorder uses view row position as queue index, every
/// row after the duplicate desynced. Each occurrence must now resolve
/// independently, in queue order.
#[test]
fn queue_window_renders_a_duplicated_id_once_per_occurrence() {
    let mut conn = seeded_conn_with_tracks(3);
    let queue_ids = vec![1, 2, 1]; // id 1 queued twice, non-adjacent

    let rows = query_track_window(
        &mut conn,
        &ViewSource::Queue,
        "ignored",
        "ignored",
        "",
        0,
        queue_ids.len() as i64,
        &queue_ids,
    )
    .unwrap();

    let ids: Vec<i64> = rows.iter().map(|t| t.id).collect();
    assert_eq!(ids, vec![1, 2, 1]);
}

/// The reviewer's specific regression: `query_track_count`'s `Queue` arm
/// and `query_track_window_queue` must agree on row count even when the
/// queue contains a duplicated id within a single window page. Before
/// this fix, count=2 (both occurrences resolve) while the window only
/// rendered 1 row (the second occurrence was dropped) — the same
/// count-versus-renderable-rows desync class Stage 3 eliminated for
/// hard-deleted ids, triggered here by an ordinary duplicate instead.
///
/// A duplicate id split across a window *boundary* (`MAX_WINDOW_LIMIT` =
/// 500) is not separately exercised here: each window call only ever
/// sees its own slice, and this fix resolves every slot in a slice
/// independently regardless of where the slice's bounds fall relative to
/// other occurrences of the same id elsewhere in `queue_ids` — a
/// duplicate straddling a page boundary is just two single-page cases
/// (one occurrence per page), each already covered by this test's same
/// per-slot, non-draining resolution.
#[test]
fn queue_count_matches_window_row_count_with_a_duplicated_id() {
    let mut conn = seeded_conn_with_tracks(3);
    let queue_ids = vec![1, 2, 1]; // id 1 queued twice

    let count = query_track_count(&conn, &ViewSource::Queue, "", &queue_ids).unwrap();
    let rows = query_track_window(
        &mut conn,
        &ViewSource::Queue,
        "ignored",
        "ignored",
        "",
        0,
        queue_ids.len() as i64,
        &queue_ids,
    )
    .unwrap();
    let ids: Vec<i64> = rows.iter().map(|t| t.id).collect();

    assert_eq!(ids, vec![1, 2, 1]);
    assert_eq!(count as usize, rows.len());
    assert_eq!(count, 3);
}

// -- ImportErrors source -------------------------------------------------

#[test]
fn import_errors_source_is_always_empty_for_now() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn.execute(
        "INSERT INTO import_errors (path, reason_kind, reason_detail, first_seen, last_seen) \
         VALUES ('/x/a.flac', 'tag', 'bad tag', 0, 0)",
        [],
    )
    .unwrap();

    let mut conn = conn;
    assert!(query_track_window(
        &mut conn,
        &ViewSource::ImportErrors,
        "x",
        "x",
        "",
        0,
        10,
        &[]
    )
    .unwrap()
    .is_empty());
    assert_eq!(
        query_track_count(&conn, &ViewSource::ImportErrors, "", &[]).unwrap(),
        0
    );
    assert!(
        query_track_ids(&conn, &ViewSource::ImportErrors, "x", "x", "", &[])
            .unwrap()
            .is_empty()
    );
}
