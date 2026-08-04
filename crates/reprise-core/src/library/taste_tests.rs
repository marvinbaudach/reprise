use rusqlite::{params, Connection};

use super::{top_genre_in, TopGenre};

/// One played track with a genre and a listening time, so a test can state
/// exactly how much of which genre this library has heard.
fn play(conn: &Connection, id: i64, genre: &str, ms_played: i64, duration_ms: i64) {
    conn.execute(
        "INSERT INTO tracks \
         (id, path, title, artist, album, genre, duration_ms, play_count, added_at) \
         VALUES (?1, ?2, 'Track', 'Artist', 'Album', ?3, ?4, 0, 0)",
        params![id, format!("/music/{id}.flac"), genre, duration_ms],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO listen_events (track_id, played_at, ms_played) VALUES (?1, 100, ?2)",
        params![id, ms_played],
    )
    .unwrap();
}

fn library() -> Connection {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate_connection(&conn).unwrap();
    conn
}

/// `RAD-5`: the suggested genre is the most *listened* one, not the one with
/// the most files — a large unplayed collection must not out-vote what the
/// library actually plays.
#[test]
fn rad_5_top_genre_follows_listening_time_not_track_count() {
    let conn = library();
    // Four short Jazz plays against one long Metal play.
    for id in 1..=4 {
        play(&conn, id, "Jazz", 30_000, 30_000);
    }
    play(&conn, 5, "Metal", 600_000, 600_000);

    let top = top_genre_in(&conn).unwrap().expect("a played genre");

    assert_eq!(top.name, "Metal");
    assert_eq!(top.tag, "metal");
}

/// Spelling variants are one genre. SQLite groups case-sensitively, so
/// without folding, "Metal" and "metal" split the very listening time that
/// should crown them and a third genre wins instead.
#[test]
fn rad_5_spelling_variants_count_as_one_genre() {
    let conn = library();
    play(&conn, 1, "Metal", 200_000, 200_000);
    play(&conn, 2, "metal", 200_000, 200_000);
    play(&conn, 3, "Jazz", 300_000, 300_000);

    let top = top_genre_in(&conn).unwrap().expect("a played genre");

    assert_eq!(
        top.name, "Metal",
        "the most-played spelling is the one worth showing"
    );
}

/// A listen never counts for more than the track's own length — the same
/// clamp the stats screen applies — so one stuck position report cannot
/// crown a genre nobody listens to.
#[test]
fn rad_5_an_overlong_listen_cannot_crown_a_genre() {
    let conn = library();
    play(&conn, 1, "Ambient", 9_000_000, 120_000);
    play(&conn, 2, "Metal", 400_000, 400_000);

    let top = top_genre_in(&conn).unwrap().expect("a played genre");

    assert_eq!(top.name, "Metal");
}

/// A multi-value genre field is one field, but a directory tags one genre at
/// a time — the search tag must be the first segment, or it matches nothing.
#[test]
fn rad_5_a_multi_value_genre_searches_by_its_first_segment() {
    let conn = library();
    play(&conn, 1, "Death Metal/Grindcore", 400_000, 400_000);

    let top = top_genre_in(&conn).unwrap().expect("a played genre");

    assert_eq!(top.name, "Death Metal");
    assert_eq!(top.tag, "death metal");
}

/// Nothing played, or nothing played that carries a genre: no suggestion at
/// all. The caller drops its chip rather than proposing a genre this library
/// has no evidence for.
#[test]
fn rad_5_a_library_without_played_genres_suggests_nothing() {
    let conn = library();
    assert_eq!(top_genre_in(&conn).unwrap(), None);

    play(&conn, 1, "   ", 400_000, 400_000);
    assert_eq!(top_genre_in(&conn).unwrap(), None);
}

/// Equal listening time must not make the chip flip between launches.
#[test]
fn rad_5_a_tie_resolves_the_same_way_every_time() {
    let conn = library();
    play(&conn, 1, "Rock", 300_000, 300_000);
    play(&conn, 2, "Jazz", 300_000, 300_000);

    assert_eq!(
        top_genre_in(&conn).unwrap(),
        Some(TopGenre {
            name: "Jazz".into(),
            tag: "jazz".into(),
        })
    );
}
