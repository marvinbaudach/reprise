//! `ViewSource::Playlist(id)` test coverage, plus `query_playlist_tracks_
//! full` (both live in `queries::playlist`) — split out of the former
//! single-file `queries.rs`'s inline test module (Refactoring &
//! Extensibility Task 1) purely to keep every file under the project's
//! 800-line rule; see `tests.rs`'s doc comment for the full split map. A
//! pure move, no assertion change.

use super::*;
use crate::library::playlists;

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
fn playlist_window_follows_position_order_by_default() {
    let mut conn = seeded_conn_with_tracks(3);
    let playlist_id = playlists::create(&conn, "P1").unwrap();
    playlists::add_tracks(&mut conn, playlist_id, &[3, 1, 2]).unwrap();

    let rows = query_track_window(
        &mut conn,
        &ViewSource::Playlist(playlist_id),
        "playlist_order",
        "asc",
        "",
        0,
        10,
        &[],
    )
    .unwrap();
    let ids: Vec<i64> = rows.iter().map(|t| t.id).collect();
    assert_eq!(ids, vec![3, 1, 2]);
}

#[test]
fn playlist_window_shows_duplicates_as_separate_rows() {
    let mut conn = seeded_conn_with_tracks(3);
    let playlist_id = playlists::create(&conn, "P1").unwrap();
    playlists::add_tracks(&mut conn, playlist_id, &[1, 2, 1]).unwrap();

    let rows = query_track_window(
        &mut conn,
        &ViewSource::Playlist(playlist_id),
        "playlist_order",
        "asc",
        "",
        0,
        10,
        &[],
    )
    .unwrap();
    let ids: Vec<i64> = rows.iter().map(|t| t.id).collect();
    assert_eq!(ids, vec![1, 2, 1]);
    assert_eq!(
        query_track_count(&conn, &ViewSource::Playlist(playlist_id), "", &[]).unwrap(),
        3
    );
}

#[test]
fn playlist_window_honors_an_explicit_column_sort_override() {
    let mut conn = seeded_conn_with_tracks(3);
    let playlist_id = playlists::create(&conn, "P1").unwrap();
    playlists::add_tracks(&mut conn, playlist_id, &[3, 1, 2]).unwrap();

    // A column header click (e.g. "title") temporarily overrides
    // playlist order.
    let rows = query_track_window(
        &mut conn,
        &ViewSource::Playlist(playlist_id),
        "title",
        "asc",
        "",
        0,
        10,
        &[],
    )
    .unwrap();
    let titles: Vec<&str> = rows.iter().map(|t| t.title.as_str()).collect();
    assert_eq!(titles, vec!["Track 1", "Track 2", "Track 3"]);
}

#[test]
fn playlist_window_excludes_missing_tracks() {
    let mut conn = seeded_conn_with_tracks(3);
    conn.execute(
        "UPDATE tracks SET missing_since = 1, missing_reason = 'unknown' WHERE id = 2",
        [],
    )
    .unwrap();
    let playlist_id = playlists::create(&conn, "P1").unwrap();
    playlists::add_tracks(&mut conn, playlist_id, &[1, 2, 3]).unwrap();

    let rows = query_track_window(
        &mut conn,
        &ViewSource::Playlist(playlist_id),
        "playlist_order",
        "asc",
        "",
        0,
        10,
        &[],
    )
    .unwrap();
    let ids: Vec<i64> = rows.iter().map(|t| t.id).collect();
    assert_eq!(ids, vec![1, 3]);
    assert_eq!(
        query_track_count(&conn, &ViewSource::Playlist(playlist_id), "", &[]).unwrap(),
        2
    );
}

#[test]
fn playlist_ids_always_follow_position_order_ignoring_sort_param() {
    let mut conn = seeded_conn_with_tracks(3);
    let playlist_id = playlists::create(&conn, "P1").unwrap();
    playlists::add_tracks(&mut conn, playlist_id, &[3, 1, 2]).unwrap();

    // Even asking for "title" order, activation ids stay position order.
    let ids = query_track_ids(
        &conn,
        &ViewSource::Playlist(playlist_id),
        "title",
        "asc",
        "",
        &[],
    )
    .unwrap();
    assert_eq!(ids, vec![3, 1, 2]);
}

#[test]
fn playlist_count_applies_filter() {
    let mut conn = seeded_conn_with_tracks(3);
    let playlist_id = playlists::create(&conn, "P1").unwrap();
    playlists::add_tracks(&mut conn, playlist_id, &[1, 2, 3]).unwrap();

    assert_eq!(
        query_track_count(&conn, &ViewSource::Playlist(playlist_id), "Track 2", &[]).unwrap(),
        1
    );
}

#[test]
fn playlist_tracks_full_returns_all_rows_in_position_order() {
    let mut conn = seeded_conn_with_tracks(5);
    let playlist_id = playlists::create(&conn, "P1").unwrap();
    playlists::add_tracks(&mut conn, playlist_id, &[3, 1, 5, 2]).unwrap();

    let tracks = query_playlist_tracks_full(&conn, playlist_id).unwrap();
    let ids: Vec<i64> = tracks.iter().map(|t| t.id).collect();
    assert_eq!(ids, vec![3, 1, 5, 2]);
}

#[test]
fn playlist_tracks_full_excludes_missing_tracks() {
    let mut conn = seeded_conn_with_tracks(3);
    conn.execute(
        "UPDATE tracks SET missing_since = 1, missing_reason = 'unknown' WHERE id = 2",
        [],
    )
    .unwrap();
    let playlist_id = playlists::create(&conn, "P1").unwrap();
    playlists::add_tracks(&mut conn, playlist_id, &[1, 2, 3]).unwrap();

    let tracks = query_playlist_tracks_full(&conn, playlist_id).unwrap();
    let ids: Vec<i64> = tracks.iter().map(|t| t.id).collect();
    assert_eq!(ids, vec![1, 3]);
}

#[test]
fn playlist_tracks_full_empty_playlist_returns_empty() {
    let conn = seeded_conn_with_tracks(3);
    let playlist_id = playlists::create(&conn, "P1").unwrap();
    let tracks = query_playlist_tracks_full(&conn, playlist_id).unwrap();
    assert!(tracks.is_empty());
}
