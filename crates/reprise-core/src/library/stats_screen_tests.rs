use rusqlite::params;

use super::{artist_rows, counts_as_play, genre_artist_rows, ranked_groups};

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
    crate::db::migrate_connection(&conn).unwrap();
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
    crate::db::migrate_connection(&conn).unwrap();
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

/// One artist, two albums: the album that sorts first by path is *not* the one
/// that was listened to. STATS-23 wants the cover of the most-played album.
#[test]
fn stats_23_representative_cover_follows_the_most_played_album() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate_connection(&conn).unwrap();
    insert_album_track(&conn, 1, "/music/Band/A Early/01.flac", "Band", "A Early");
    insert_album_track(&conn, 2, "/music/Band/Z Later/01.flac", "Band", "Z Later");
    play(&conn, 1, 100);
    for at in [200, 300, 400] {
        play(&conn, 2, at);
    }

    let ranked = ranked_groups(&artist_rows(&conn, 0, 1_000).unwrap());

    assert_eq!(ranked.len(), 1);
    assert_eq!(
        ranked[0].representative_track_path,
        "/music/Band/Z Later/01.flac"
    );
    assert_eq!(
        ranked[0].cover_candidates,
        vec![
            "/music/Band/Z Later/01.flac".to_string(),
            "/music/Band/A Early/01.flac".to_string(),
        ],
        "the runner-up album stays available for a cover that does not resolve"
    );
}

/// A cover candidate list of four albums is cut to three: the view walks it
/// synchronously, and a long walk would keep the card blank.
#[test]
fn stats_23_cover_candidates_stop_at_three() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate_connection(&conn).unwrap();
    for (id, album) in [(1, "D"), (2, "C"), (3, "B"), (4, "A")] {
        insert_album_track(
            &conn,
            id,
            &format!("/music/Band/{album}/01.flac"),
            "Band",
            album,
        );
        for play_index in 0..id {
            play(&conn, id, 100 + play_index * 10);
        }
    }

    let ranked = ranked_groups(&artist_rows(&conn, 0, 1_000).unwrap());

    assert_eq!(ranked[0].cover_candidates.len(), 3);
    assert_eq!(ranked[0].cover_candidates[0], "/music/Band/A/01.flac");
}

/// Grouping the query one level finer must not move a single play between
/// artists, and must not change which spelling wins the label.
#[test]
fn stats_23_album_grouping_leaves_plays_and_labels_untouched() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate_connection(&conn).unwrap();
    // "Band" is spelled twice; the dominant spelling has its plays spread over
    // three albums, the other has all of them on one.
    for (id, album) in [(1, "One"), (2, "Two"), (3, "Three")] {
        insert_album_track(&conn, id, &format!("/music/a/{id}.flac"), "Band", album);
        for play_index in 0..2 {
            play(&conn, id, 100 + i64::from(play_index));
        }
    }
    insert_album_track(&conn, 4, "/music/b/4.flac", "band ", "Four");
    for play_index in 0..5 {
        play(&conn, 4, 200 + i64::from(play_index));
    }

    let ranked = ranked_groups(&artist_rows(&conn, 0, 1_000).unwrap());

    assert_eq!(ranked.len(), 1, "both spellings fold into one group");
    assert_eq!(ranked[0].group.plays, 11);
    assert_eq!(ranked[0].group.ms, 1_100_000);
    assert_eq!(
        ranked[0].group.label, "Band",
        "the label follows summed plays per spelling, not per album row"
    );
}

#[test]
fn stats_23_equivalent_album_spellings_form_one_cover_candidate() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate_connection(&conn).unwrap();
    for (id, path, album, plays) in [
        (1, "/music/Band/Shared/01.flac", "Shared Album", 3),
        (2, "/music/Band/Shared/02.flac", " shared album ", 3),
        (3, "/music/Band/A Other/01.flac", "Other", 4),
    ] {
        insert_album_track(&conn, id, path, "Band", album);
        for play_index in 0..plays {
            play(&conn, id, 100 + play_index);
        }
    }

    let ranked = ranked_groups(&artist_rows(&conn, 0, 1_000).unwrap());

    assert_eq!(
        ranked[0].representative_track_path, "/music/Band/Shared/01.flac",
        "equivalent album spellings must combine before candidates are ranked"
    );
    assert_eq!(ranked[0].cover_candidates.len(), 2);
}

#[test]
fn stats_23_album_ties_use_listening_time_then_path() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate_connection(&conn).unwrap();
    for (id, path, album, ms) in [
        (1, "/music/Band/Z Time/01.flac", "Z Time", 100_000),
        (2, "/music/Band/B Path/01.flac", "B Path", 50_000),
        (3, "/music/Band/A Path/01.flac", "A Path", 50_000),
    ] {
        insert_album_track(&conn, id, path, "Band", album);
        for played_at in [100, 200] {
            play_ms(&conn, id, played_at + id, ms);
        }
    }

    let ranked = ranked_groups(&artist_rows(&conn, 0, 1_000).unwrap());

    assert_eq!(
        ranked[0].cover_candidates,
        vec![
            "/music/Band/Z Time/01.flac".to_string(),
            "/music/Band/A Path/01.flac".to_string(),
            "/music/Band/B Path/01.flac".to_string(),
        ]
    );
}

fn insert_album_track(conn: &rusqlite::Connection, id: i64, path: &str, artist: &str, album: &str) {
    conn.execute(
        "INSERT INTO tracks \
         (id, path, title, artist, album, album_artist, genre, duration_ms, \
          play_count, added_at) \
         VALUES (?1, ?2, 'Track', ?3, ?4, '', 'Rock', 100000, 0, 0)",
        rusqlite::params![id, path, artist, album],
    )
    .unwrap();
}

fn play(conn: &rusqlite::Connection, track_id: i64, played_at: i64) {
    play_ms(conn, track_id, played_at, 100_000);
}

fn play_ms(conn: &rusqlite::Connection, track_id: i64, played_at: i64, ms_played: i64) {
    conn.execute(
        "INSERT INTO listen_events (track_id, played_at, ms_played) \
         VALUES (?1, ?2, ?3)",
        rusqlite::params![track_id, played_at, ms_played],
    )
    .unwrap();
}
