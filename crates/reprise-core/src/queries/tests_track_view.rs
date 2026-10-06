use super::*;
use crate::library::playlists;

fn seeded_track_views() -> (crate::db::Db, i64, Vec<QueueItem>) {
    let db = crate::db::Db::open_in_memory().unwrap();
    let conn = db.conn();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    for (id, title, genre, added_at) in [
        (1, "Keep One", "Rock", now),
        (2, "Keep Two", "Rock", now),
        (3, "Drop by text", "Rock", now),
        (4, "Keep Jazz", "Jazz", now),
        (5, "Keep Old", "Rock", 0),
    ] {
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, genre, added_at) \
             VALUES (?1, ?2, ?3, 'Artist', ?4, ?5)",
            rusqlite::params![id, format!("/music/{id}.flac"), title, genre, added_at],
        )
        .unwrap();
    }

    let playlist_id = playlists::create(&db, "Track view invariant").unwrap();
    playlists::add_tracks(&db, playlist_id, &[1, 2, 3, 4, 5]).unwrap();
    let queue_items = [1, 2, 3, 4, 5].into_iter().map(QueueItem::Track).collect();
    (db, playlist_id, queue_items)
}

fn assert_query_family_agrees(db: &Db, view: &TrackViewQuery<'_>, expected_count: usize) {
    let sort = match view.source {
        ViewSource::Playlist(_) => test_sort("playlist_order", "asc"),
        _ => test_sort("title", "asc"),
    };
    let count = query_track_count(db, view).unwrap();
    let ids = query_track_ids(db, view, sort).unwrap();
    let rows = query_track_window(
        db,
        view,
        sort,
        test_rows(0, MAX_WINDOW_LIMIT),
        AiColumn::Project,
    )
    .unwrap();

    assert_eq!(count as usize, expected_count);
    assert_eq!(ids.len(), expected_count);
    assert_eq!(rows.len(), expected_count);
    assert_eq!(ids, rows.iter().map(|track| track.id).collect::<Vec<_>>());
}

#[test]
fn track_view_query_defaults_are_empty() {
    let source = ViewSource::Library;
    let view = TrackViewQuery::new(&source);

    assert_eq!(view.filter, "");
    assert_eq!(view.browse, &BrowseFilter::default());
    assert!(view.queue_items.is_empty());
    assert!(!view.exclude_ai);
}

#[test]
fn track_view_query_family_agrees_for_each_track_source_shape() {
    let (db, playlist_id, queue_items) = seeded_track_views();
    let browse = BrowseFilter {
        genre: Some("Rock".into()),
        ..BrowseFilter::default()
    };
    let library = ViewSource::Library;
    let recently_added = ViewSource::RecentlyAdded;
    let playlist = ViewSource::Playlist(playlist_id);
    let queue = ViewSource::Queue;

    assert_query_family_agrees(
        &db,
        &TrackViewQuery::new(&library)
            .with_filter("Keep")
            .with_browse(&browse),
        3,
    );
    assert_query_family_agrees(
        &db,
        &TrackViewQuery::new(&recently_added)
            .with_filter("Keep")
            .with_browse(&browse),
        2,
    );
    assert_query_family_agrees(&db, &TrackViewQuery::new(&playlist).with_filter("Keep"), 4);
    assert_query_family_agrees(
        &db,
        &TrackViewQuery::new(&queue).with_queue_items(&queue_items),
        5,
    );
}

#[test]
fn excluding_ai_hides_provenance_flagged_library_tracks() {
    let (db, _, _) = seeded_track_views();
    db.conn()
        .execute(
            "INSERT INTO track_provenance (track_id, kind, ai, created_at) \
             VALUES (2, 'vocals-removed', 1, 0)",
            [],
        )
        .unwrap();
    let source = ViewSource::Library;
    let view = TrackViewQuery::new(&source)
        .with_filter("Keep")
        .with_exclude_ai(true);
    let ids = query_track_ids(&db, &view, test_sort("title", "asc")).unwrap();

    assert!(!ids.contains(&2));
    assert_eq!(ids, vec![4, 5, 1]);
}
