// Rule-named acceptance tests for docs/ux-rules.md. Each [aktiv] rule in
// the rulebook has at least one test here; scripts/check-ux-traceability.sh
// gates the mapping. One primary rule ID per test name.

use super::*;

// UX PLAY-2: double-click plays the row and appends the rest of the visible
// list from that position onto the queue (activation snapshot).
#[test]
fn play_2_activation_snapshot_starts_at_clicked_row() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30, 40], 2);
    assert_eq!(q.current(), Some(30));
    assert_eq!(q.advance_auto(), Some(40));
    assert_eq!(
        q.advance_auto(),
        None,
        "tracks before the clicked row never follow automatically (Repeat::Off)"
    );
}

// UX PLAY-3a: the queue is a snapshot of the filtered hits; shuffle permutes
// exactly those hits (queue = hit set, no track from outside).
#[test]
fn play_3a_shuffle_stays_inside_filtered_snapshot() {
    let mut q = Queue::new();
    let hits = vec![11, 22, 33, 44, 55];
    q.set_tracks(hits.clone(), 0);
    q.set_shuffle(true);
    let mut queue_ids = q.ids_in_order();
    queue_ids.sort_unstable();
    assert_eq!(queue_ids, hits);
    assert_eq!(
        q.current(),
        Some(11),
        "the current track stays put when shuffle is toggled"
    );
}

// UX PLAY-4a: list playback never seeds missing rows, and a track that goes
// missing after it was queued is skipped silently in either pending layer.
#[test]
fn play_4a_list_playback_and_queue_advance_skip_missing_silently() {
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    for (id, missing_since) in [(1_i64, None), (2, Some(1_i64)), (3, None)] {
        conn.execute(
            "INSERT INTO tracks \
             (id, path, title, artist, added_at, missing_since, missing_reason) \
             VALUES (?1, ?2, ?3, 'Artist', 0, ?4, \
                     CASE WHEN ?4 IS NULL THEN NULL ELSE 'deleted' END)",
            rusqlite::params![
                id,
                format!("/x/{id}.flac"),
                format!("Track {id}"),
                missing_since
            ],
        )
        .unwrap();
    }
    let playlist_id = crate::library::playlists::create(&conn, "P1").unwrap();
    crate::library::playlists::add_tracks(&mut conn, playlist_id, &[1, 2, 3]).unwrap();
    let playable = crate::queries::query_track_ids(
        &conn,
        &crate::view_source::ViewSource::Playlist(playlist_id),
        "playlist_order",
        "asc",
        "",
        &[],
    )
    .unwrap();
    assert_eq!(playable, vec![1, 3], "Play all skips missing list rows");
    let mut shuffled = Queue::new();
    shuffled.set_tracks(playable.clone(), 0);
    shuffled.set_shuffle(true);
    let mut shuffled_ids = shuffled.ids_in_order();
    shuffled_ids.sort_unstable();
    assert_eq!(
        shuffled_ids,
        vec![1, 3],
        "Shuffle uses the same playable set"
    );

    let mut context = Queue::new();
    context.set_tracks(vec![1, 2, 2, 3], 0);
    assert_eq!(context.peek_auto_matching(|id| id != 2), Some(3));
    assert_eq!(
        context.current(),
        Some(1),
        "pre-feed must not move the playhead"
    );
    assert_eq!(
        context.advance_auto_matching(|id| id != 2),
        Some(3),
        "every row that became missing after queue creation is skipped"
    );
    assert_eq!(context.ids_in_order(), vec![1, 2, 2, 3]);

    let mut unavailable = Queue::new();
    unavailable.set_tracks(vec![1, 2, 3], 0);
    unavailable.set_repeat(Repeat::All);
    assert_eq!(unavailable.advance_auto_matching(|_| false), None);
    assert_eq!(
        unavailable.ids_in_order(),
        vec![1, 2, 3],
        "skipped unavailable entries stay in the durable queue"
    );

    let mut pending = crate::up_next::UpNextQueue::default();
    pending.append(&[2, 2, 3]);
    assert_eq!(pending.take_first_matching(|id| id != 2), Some(3));
    assert_eq!(pending.ids(), &[2, 2]);
}

// UX PLAY-5a: externally deleted tracks leave the queue silently; the
// playing track stays untouched.
#[test]
fn play_5a_deleted_tracks_leave_queue_silently() {
    let mut q = Queue::new();
    q.set_tracks(vec![1, 2, 3, 4], 1);
    assert!(q.remove_ids(&[3]));
    assert_eq!(q.ids_in_order(), vec![1, 2, 4]);
    assert_eq!(
        q.current(),
        Some(2),
        "background removal never stops the playing track"
    );
}

// UX QUE-1 [geplant] — demo of the activation workflow. The three-section
// queue itself shipped on main (c5200e1), but this core stub cannot prove
// the sections; the flip needs a [gtk] test that can. Whoever writes it
// removes the #[ignore] and flips QUE-1 to [aktiv] in the same commit.
#[test]
#[ignore = "UX QUE-1 [geplant] — needs a [gtk] section test; this core stub cannot prove the three sections"]
fn que_1_queue_is_never_empty_while_playing() {
    let mut q = Queue::new();
    q.set_tracks(vec![7, 8, 9], 0);
    assert!(
        !q.is_empty(),
        "while something is playing the queue is never empty"
    );
}
