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

// UX PLAY-5a: scan-detected deleted tracks leave both queue layers silently;
// unmounted tracks and the playing track remain untouched.
#[test]
fn play_5a_scan_detection_purges_deleted_but_retains_unmounted_and_playing_tracks() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    std::fs::create_dir(&root).unwrap();
    let deleted_path = root.join("deleted.flac");
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac");
    std::fs::copy(fixture, &deleted_path).unwrap();
    let mut conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    let initial = crate::library::scanner::scan_folder(&mut conn, &root).unwrap();
    assert!(matches!(
        initial,
        crate::library::scanner::ScanOutcome::Completed(_)
    ));
    let deleted_id: i64 = conn
        .query_row(
            "SELECT id FROM tracks WHERE path = ?1",
            [deleted_path.to_string_lossy().as_ref()],
            |row| row.get(0),
        )
        .unwrap();
    let unmounted_id = deleted_id + 1;
    let playing_id = deleted_id + 2;
    for (id, path, missing_reason) in [
        (unmounted_id, root.join("unmounted.flac"), Some("unmounted")),
        (playing_id, temp.path().join("playing.flac"), None),
    ] {
        conn.execute(
            "INSERT INTO tracks \
             (id, path, title, artist, added_at, missing_since, missing_reason) \
             VALUES (?1, ?2, ?3, 'Artist', 0, \
                     CASE WHEN ?4 IS NULL THEN NULL ELSE 1 END, ?4)",
            rusqlite::params![
                id,
                path.to_string_lossy(),
                format!("Track {id}"),
                missing_reason
            ],
        )
        .unwrap();
    }

    std::fs::remove_file(&deleted_path).unwrap();
    let report = match crate::library::scanner::scan_folder(&mut conn, &root).unwrap() {
        crate::library::scanner::ScanOutcome::Completed(report) => report,
        crate::library::scanner::ScanOutcome::RootUnavailable { root } => {
            panic!("test root unexpectedly unavailable: {}", root.display())
        }
    };
    assert_eq!(report.vanished, 1);
    let reasons: Vec<(i64, Option<String>)> = {
        let mut statement = conn
            .prepare("SELECT id, missing_reason FROM tracks ORDER BY id")
            .unwrap();
        statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    assert_eq!(
        reasons,
        vec![
            (deleted_id, Some("deleted".into())),
            (unmounted_id, Some("unmounted".into())),
            (playing_id, None)
        ]
    );
    let auto_cleaned =
        crate::library::scanner::finalize_completed_scan(&mut conn, &report, 10).unwrap();
    assert!(auto_cleaned.is_empty(), "auto-clean is disabled by default");
    let mut queue = Queue::new();
    queue.set_tracks(vec![playing_id, deleted_id, unmounted_id], 0);
    let mut pending = crate::up_next::UpNextQueue::default();
    pending.append(&[deleted_id, unmounted_id]);
    let mut candidates = queue.ids_in_order();
    candidates.extend_from_slice(pending.ids());
    let mut purge = auto_cleaned;
    purge.extend(crate::queries::query_queue_purge_track_ids(&conn, &candidates).unwrap());
    queue.remove_ids(&purge);
    pending.remove_ids(&purge);

    assert_eq!(queue.ids_in_order(), vec![playing_id, unmounted_id]);
    assert_eq!(pending.ids(), &[unmounted_id]);
    assert_eq!(queue.current(), Some(playing_id));
}

// UX PLAY-5b: an unavailable mount never destroys queue order or interrupts
// the current track; advance skips it, and clearing the missing state makes
// the retained id playable again.
#[test]
fn play_5b_unmounted_tracks_stay_skip_heal_and_never_stop_current() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    for (id, reason, removed_at) in [
        (1_i64, None, None),
        (2, Some("unmounted"), None),
        (3, Some("deleted"), None),
        (4, Some("unknown"), None),
        (5, Some("unmounted"), Some(9_i64)),
    ] {
        conn.execute(
            "INSERT INTO tracks \
             (id, path, title, artist, added_at, missing_since, missing_reason, removed_at) \
             VALUES (?1, ?2, ?3, 'Artist', 0, \
                     CASE WHEN ?4 IS NULL THEN NULL ELSE 1 END, ?4, ?5)",
            rusqlite::params![
                id,
                format!("/x/{id}.flac"),
                format!("Track {id}"),
                reason,
                removed_at
            ],
        )
        .unwrap();
    }

    let retained = crate::queries::query_queue_retained_track_ids(&conn).unwrap();
    assert_eq!(retained, std::collections::HashSet::from([1, 2, 4]));

    let mut queue = Queue::new();
    queue.set_tracks(vec![1, 2, 3], 0);
    let purge: Vec<_> = queue
        .ids_in_order()
        .into_iter()
        .filter(|id| !retained.contains(id))
        .collect();
    queue.remove_ids(&purge);
    assert_eq!(queue.ids_in_order(), vec![1, 2]);
    assert_eq!(
        queue.current(),
        Some(1),
        "background availability changes never stop the playing track"
    );

    let live = crate::queries::query_live_track_ids(&conn).unwrap();
    assert_eq!(queue.advance_auto_matching(|id| live.contains(&id)), None);
    assert_eq!(queue.ids_in_order(), vec![1, 2]);

    conn.execute(
        "UPDATE tracks SET missing_since = NULL, missing_reason = NULL WHERE id = 2",
        [],
    )
    .unwrap();
    let healed = crate::queries::query_live_track_ids(&conn).unwrap();
    let mut healed_queue = Queue::new();
    healed_queue.set_tracks(queue.ids_in_order(), 0);
    assert_eq!(
        healed_queue.advance_auto_matching(|id| healed.contains(&id)),
        Some(2)
    );
}
