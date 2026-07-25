use rusqlite::params;

use super::{counts_as_play, genre_artist_rows};

#[test]
fn counts_as_play_matches_the_scrobble_threshold() {
    assert!(!counts_as_play(89_999, 180_000));
    assert!(counts_as_play(90_000, 180_000));
    assert!(!counts_as_play(239_999, 600_000));
    assert!(counts_as_play(240_000, 600_000));
    assert!(!counts_as_play(1_000, 0));
    assert!(!counts_as_play(1_000, -1));
}

#[test]
fn genre_artist_rows_exclude_blank_genres() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    for (id, genre) in [(1, "Deathcore"), (2, "  ")] {
        conn.execute(
            "INSERT INTO tracks \
             (id, path, title, artist, album, genre, duration_ms, play_count, added_at) \
             VALUES (?1, ?2, 'Track', 'Artist', 'Album', ?3, 100000, 0, 0)",
            params![id, format!("/music/{id}.flac"), genre],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO listen_events (track_id, played_at, ms_played) \
             VALUES (?1, 100, 100000)",
            [id],
        )
        .unwrap();
    }

    let rows = genre_artist_rows(&conn, 0, 200).unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].genre_raw, "Deathcore");
}

#[test]
fn genre_artist_rows_aggregate_tracks_for_the_same_genre_artist() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    for (id, path, played_at, ms_played) in [
        (1, "/music/a.flac", 100, 40_000),
        (2, "/music/z.flac", 101, 60_000),
    ] {
        conn.execute(
            "INSERT INTO tracks \
             (id, path, title, artist, album, genre, duration_ms, play_count, added_at) \
             VALUES (?1, ?2, 'Track', 'Artist', 'Album', 'Deathcore', 100000, 0, 0)",
            params![id, path],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO listen_events (track_id, played_at, ms_played) \
             VALUES (?1, ?2, ?3)",
            params![id, played_at, ms_played],
        )
        .unwrap();
    }

    let rows = genre_artist_rows(&conn, 0, 200).unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].artist.plays, 2);
    assert_eq!(rows[0].artist.ms, 100_000);
    assert_eq!(rows[0].artist.last_played_at, 101);
    assert_eq!(rows[0].artist.path, "/music/z.flac");
}
