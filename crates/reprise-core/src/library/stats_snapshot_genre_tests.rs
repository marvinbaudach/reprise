use chrono::{TimeZone, Utc};
use rusqlite::params;

use super::compute;
use crate::library::stats_period::StatsPeriod;

const NOW_2026_07_19: i64 = 1_784_424_000;

#[test]
fn stats_15_genre_top_artist_uses_group_key() {
    let conn = migrated_conn();
    insert_track(&conn, 1, "Lorna A", "Lorna Shore", "Deathcore");
    insert_track(&conn, 2, "Lorna B", "lorna shore", "deathcore");
    insert_track(&conn, 3, "Rival", "Rival", "Deathcore");
    for (track_id, plays) in [(1, 3), (2, 2), (3, 4)] {
        for minute in 0..plays {
            insert_event(&conn, track_id, minute);
        }
    }

    let snapshot = compute(&conn, StatsPeriod::Year(2026), NOW_2026_07_19, &Utc).unwrap();
    let deathcore = snapshot
        .genres
        .segments
        .iter()
        .find(|segment| segment.key == "name:deathcore")
        .unwrap();

    assert_eq!(deathcore.top_artist.as_deref(), Some("Lorna Shore"));
    assert_eq!(deathcore.representative_track_path, "/music/3-rival.flac");
}

#[test]
fn stats_15_other_has_no_artist_or_representative_path() {
    let conn = migrated_conn();
    for (index, genre) in ["Rock", "Jazz", "Folk", "Pop", "Metal", "Soul"]
        .into_iter()
        .enumerate()
    {
        let id = index as i64 + 1;
        insert_track(&conn, id, genre, genre, genre);
        insert_event(&conn, id, 0);
    }

    let snapshot = compute(&conn, StatsPeriod::Year(2026), NOW_2026_07_19, &Utc).unwrap();
    let other = snapshot.genres.segments.last().unwrap();

    assert_eq!(other.label, "Other");
    assert_eq!(other.top_artist, None);
    assert!(other.representative_track_path.is_empty());
}

fn migrated_conn() -> crate::db::Db {
    crate::db::Db::open_in_memory().unwrap()
}

fn insert_track(conn: &crate::db::Db, id: i64, title: &str, artist: &str, genre: &str) {
    conn.conn()
        .execute(
            "INSERT INTO tracks \
         (id, path, title, artist, album, duration_ms, play_count, added_at) \
         VALUES (?1, ?2, ?3, ?4, 'Album', 100000, 0, 0)",
            params![
                id,
                format!("/music/{id}-{}.flac", title.to_lowercase()),
                title,
                artist
            ],
        )
        .unwrap();
    conn.conn()
        .execute(
            "UPDATE tracks SET genre = ?1 WHERE id = ?2",
            params![genre, id],
        )
        .unwrap();
}

fn insert_event(conn: &crate::db::Db, track_id: i64, minute: i64) {
    let played_at = Utc
        .with_ymd_and_hms(2026, 3, track_id as u32, 12, minute as u32, 0)
        .single()
        .unwrap()
        .timestamp();
    conn.conn()
        .execute(
            "INSERT INTO listen_events (track_id, played_at, ms_played) VALUES (?1, ?2, 100000)",
            params![track_id, played_at],
        )
        .unwrap();
}
