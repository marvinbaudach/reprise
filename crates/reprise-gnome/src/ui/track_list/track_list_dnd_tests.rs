use super::*;

// ## format_drag_payload / parse_drag_payload round trips.

#[test]
fn que_9_round_trips_typed_items_without_losing_colliding_ids() {
    let items = [
        QueueItem::Track(7),
        QueueItem::Episode(7),
        QueueItem::Track(3),
    ];
    let payload = format_drag_payload(&items, None);
    assert_eq!(payload, "t7,e7,t3|-");
    let parsed = parse_drag_payload(&payload).unwrap();
    assert_eq!(parsed.items, items);
    assert_eq!(parsed.reorder_position, None);
}

#[test]
fn round_trips_single_id_payload_with_reorder_position() {
    let payload = format_drag_payload(&[QueueItem::Track(42)], Some(7));
    assert_eq!(payload, "t42|7");
    let parsed = parse_drag_payload(&payload).unwrap();
    assert_eq!(parsed.items, vec![QueueItem::Track(42)]);
    assert_eq!(parsed.reorder_position, Some(7));
}

#[test]
fn parse_rejects_payload_with_no_separator() {
    assert!(parse_drag_payload("1,2,3").is_none());
}

#[test]
fn parse_rejects_empty_ids_half() {
    assert!(parse_drag_payload("|-").is_none());
}

#[test]
fn parse_rejects_non_numeric_ids() {
    assert!(parse_drag_payload("abc|-").is_none());
}

#[test]
fn parse_rejects_legacy_untyped_ids_instead_of_guessing_track_kind() {
    assert!(parse_drag_payload("42|-").is_none());
    assert!(parse_drag_payload("7,8|-").is_none());
}

#[test]
fn parse_rejects_non_numeric_non_dash_position() {
    assert!(parse_drag_payload("t1|abc").is_none());
}

#[test]
fn parse_rejects_a_foreign_plain_string_without_a_pipe() {
    // Guards against a drop from outside this app (e.g. a browser tab's
    // dragged link text) being misread as a valid payload.
    assert!(parse_drag_payload("https://example.com/").is_none());
}

// ## reorder_position_for_drag

fn seeded_playlist_model(track_ids_in_order: &[i64]) -> (TrackListModel, i64) {
    let conn = crate::test_db::open().unwrap();
    for id in track_ids_in_order {
        crate::test_db::connection(&conn)
            .execute(
                "INSERT INTO tracks (id, path, title, artist, added_at) VALUES (?1, ?2, ?3, '', 0)",
                rusqlite::params![id, format!("/x/{id}.flac"), format!("Track {id}")],
            )
            .unwrap();
    }
    let conn = std::rc::Rc::new(conn);
    let playlist_id = playlists::create(&conn, "P1").unwrap();
    playlists::add_tracks(&conn, playlist_id, track_ids_in_order).unwrap();
    let model = TrackListModel::new(conn);
    model.set_query(
        &ViewSource::Playlist(playlist_id),
        "playlist_order",
        "asc",
        "",
        &[],
    );
    (model, playlist_id)
}

#[test]
fn reorder_position_for_queue_is_always_the_view_position() {
    let (model, _) = seeded_playlist_model(&[10, 20, 30]);
    // Queue never reads `model` at all for this decision — any model works,
    // including one queried over an unrelated playlist.
    assert_eq!(
        reorder_position_for_drag(&model, &ViewSource::Queue, false, 5),
        Some(5)
    );
    assert_eq!(
        reorder_position_for_drag(&model, &ViewSource::Queue, true, 0),
        Some(0)
    );
}

#[test]
fn reorder_position_for_playlist_reads_true_position_when_allowed() {
    let (model, playlist_id) = seeded_playlist_model(&[10, 20, 30]);
    // pt.position 1 holds track id 20 (0-indexed insertion order).
    let pos = reorder_position_for_drag(&model, &ViewSource::Playlist(playlist_id), true, 1);
    assert_eq!(pos, Some(1));
}

#[test]
fn reorder_position_for_playlist_is_none_when_not_allowed() {
    let (model, playlist_id) = seeded_playlist_model(&[10, 20, 30]);
    let pos = reorder_position_for_drag(&model, &ViewSource::Playlist(playlist_id), false, 1);
    assert_eq!(
        pos, None,
        "a sorted/filtered playlist view must never resolve a reorder position"
    );
}

#[test]
fn reorder_position_for_library_is_always_none() {
    let (model, _) = seeded_playlist_model(&[10, 20, 30]);
    assert_eq!(
        reorder_position_for_drag(&model, &ViewSource::Library, true, 0),
        None
    );
}

/// Proof this doesn't regress into Task 5's bug: under a divergent
/// (artist-sorted) view, `reorder_position_for_drag` must still return the
/// TRUE `pt.position` when `playlist_reorder_allowed` is (falsely) passed as
/// `true` — asserting the raw view index is never silently substituted. The
/// caller (`playlist_reorder_allowed`) is what's actually responsible for
/// passing `false` in that state; this test pins the position-lookup half of
/// the contract in isolation.
#[test]
fn reorder_position_for_playlist_uses_true_position_not_view_index_under_a_sort() {
    let conn = crate::test_db::open().unwrap();
    // Insertion order (== pt.position order) is A, B, C; artist-ascending
    // view order is C, A, B (artists Alpha, Zeta, ... chosen to diverge).
    let tracks = [(1, "A", "Zeta"), (2, "B", "Theta"), (3, "C", "Alpha")];
    for (id, title, artist) in tracks {
        crate::test_db::connection(&conn)
            .execute(
                "INSERT INTO tracks (id, path, title, artist, added_at) VALUES (?1, ?2, ?3, ?4, 0)",
                rusqlite::params![id, format!("/x/{id}.flac"), title, artist],
            )
            .unwrap();
    }
    let playlist_id = playlists::create(&conn, "P1").unwrap();
    let conn = std::rc::Rc::new(conn);
    playlists::add_tracks(&conn, playlist_id, &[1, 2, 3]).unwrap();
    let model = TrackListModel::new(conn);
    model.set_query(&ViewSource::Playlist(playlist_id), "artist", "asc", "", &[]);
    // View row 0 is track C (id 3), whose true pt.position is 2.
    assert_eq!(model.track_at(0).unwrap().id, 3);

    let pos = reorder_position_for_drag(&model, &ViewSource::Playlist(playlist_id), true, 0);
    assert_eq!(
        pos,
        Some(2),
        "must resolve track C's true pt.position (2), not its view index (0)"
    );
}

// ## resolve_reorder_target

#[test]
fn resolve_reorder_target_single_row_different_position() {
    let payload = DragPayload {
        items: vec![QueueItem::Track(7)],
        reorder_position: Some(2),
    };
    let result = resolve_reorder_target(&payload, 5).unwrap();
    assert_eq!(result, ReorderMove { from: 2, to: 5 });
}

#[test]
fn resolve_reorder_target_rejects_multi_row_payload() {
    let payload = DragPayload {
        items: vec![QueueItem::Track(7), QueueItem::Track(8)],
        reorder_position: None,
    };
    assert!(resolve_reorder_target(&payload, 5).is_none());
}

#[test]
fn resolve_reorder_target_rejects_a_payload_with_no_reorder_position() {
    let payload = DragPayload {
        items: vec![QueueItem::Track(7)],
        reorder_position: None,
    };
    assert!(resolve_reorder_target(&payload, 5).is_none());
}

#[test]
fn resolve_reorder_target_rejects_dropping_a_row_onto_itself() {
    let payload = DragPayload {
        items: vec![QueueItem::Track(7)],
        reorder_position: Some(3),
    };
    assert!(resolve_reorder_target(&payload, 3).is_none());
}
