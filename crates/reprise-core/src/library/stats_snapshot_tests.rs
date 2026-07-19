use chrono::{FixedOffset, MappedLocalTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
use rusqlite::{params, Connection};

use super::{compute, SortBy};
use crate::library::group_key::GroupKind;
use crate::library::stats_period::{Granularity, StatsPeriod};
use crate::library::stats_screen::group_track_ids;

const NOW_2026_07_19: i64 = 1_784_424_000;

#[test]
fn stats_0_play_definition_consistent_time_and_count() {
    let conn = migrated_conn();
    insert_track(&conn, 1, "Short", "One", "", "Rock", 100_000, 40, None);
    insert_track(&conn, 2, "Long", "Two", "", "Jazz", 200_000, 90, None);
    insert_track(&conn, 3, "Unknown", "Three", "", "Folk", 0, 70, None);
    insert_event(&conn, 1, timestamp(2026, 1, 2, 12, 0), 150_000);
    insert_event(&conn, 2, timestamp(2026, 1, 3, 12, 0), 180_000);
    insert_event(&conn, 3, timestamp(2026, 1, 4, 12, 0), 50_000);

    let snapshot = compute(&conn, StatsPeriod::AllTime, NOW_2026_07_19, &Utc).unwrap();

    assert_eq!(snapshot.hero.total_ms, 330_000);
    assert_eq!(snapshot.hero.plays, 3);
    assert_eq!(
        snapshot
            .top_tracks
            .iter()
            .map(|track| track.play_count)
            .sum::<i64>(),
        snapshot.hero.plays
    );
}

#[test]
fn stats_6_sparse_uses_finer_granularity() {
    let sparse = migrated_conn();
    insert_track(&sparse, 1, "Sparse", "Artist", "", "Rock", 60_000, 0, None);
    for (day, count) in [(1, 2), (5, 1), (10, 1), (15, 1)] {
        for offset in 0..count {
            insert_event(&sparse, 1, timestamp(2026, 1, day, 12, offset), 30_000);
        }
    }
    let sparse_snapshot =
        compute(&sparse, StatsPeriod::YearToDate(2026), NOW_2026_07_19, &Utc).unwrap();
    assert_eq!(sparse_snapshot.period.granularity, Granularity::Day);

    let dense = migrated_conn();
    insert_track(&dense, 1, "Dense", "Artist", "", "Rock", 60_000, 0, None);
    let start = timestamp(2026, 1, 1, 12, 0);
    for index in 0..30 {
        insert_event(&dense, 1, start + index * 6 * 86_400, 30_000);
    }
    let dense_snapshot =
        compute(&dense, StatsPeriod::YearToDate(2026), NOW_2026_07_19, &Utc).unwrap();
    assert_eq!(dense_snapshot.period.granularity, Granularity::Month);

    let empty = migrated_conn();
    let empty_snapshot =
        compute(&empty, StatsPeriod::YearToDate(2026), NOW_2026_07_19, &Utc).unwrap();
    assert!(empty_snapshot.is_empty());
    assert!(empty_snapshot.period.buckets.is_empty());
}

#[test]
fn stats_2_spotlight_reports_share_and_top_tracks() {
    let conn = migrated_conn();
    let artists = [
        ("Alpha", 5),
        ("Beta", 4),
        ("Gamma", 3),
        ("Delta", 2),
        ("Epsilon", 1),
    ];
    let mut track_id = 1;
    for (artist, plays) in artists {
        let tracks = if artist == "Alpha" { 3 } else { 1 };
        for track_index in 0..tracks {
            insert_track(
                &conn,
                track_id,
                &format!("{artist} {track_index}"),
                artist,
                "",
                "Rock",
                100_000,
                0,
                None,
            );
            let track_plays = if artist == "Alpha" {
                [2, 2, 1][track_index]
            } else {
                plays
            };
            for play in 0..track_plays {
                insert_event(
                    &conn,
                    track_id,
                    timestamp(2026, 2, track_id as u32, 12, play),
                    100_000,
                );
            }
            track_id += 1;
        }
    }

    let snapshot = compute(&conn, StatsPeriod::Year(2026), NOW_2026_07_19, &Utc).unwrap();
    let spotlight = snapshot.spotlight.as_ref().unwrap();

    assert_eq!(spotlight.artist.group.label, "Alpha");
    assert_eq!(spotlight.artist.group.plays, 5);
    assert_eq!(spotlight.artist.group.ms, 500_000);
    assert_eq!(spotlight.share_percent, 33);
    assert_eq!(spotlight.top_tracks.len(), 3);
    assert_eq!(spotlight.also.len(), 4);
}

#[test]
fn stats_3_genre_spectrum_buckets_other() {
    let conn = migrated_conn();
    for (index, genre) in ["Rock", "Jazz", "Folk", "Pop", "Metal", "Soul", "Punk"]
        .into_iter()
        .enumerate()
    {
        let id = index as i64 + 1;
        insert_track(&conn, id, genre, "Artist", "", genre, 1_000, 0, None);
        insert_event(
            &conn,
            id,
            timestamp(2026, 3, id as u32, 12, 0),
            (8 - id) * 100,
        );
    }
    insert_track(&conn, 20, "Blank", "Artist", "", "", 10_000, 0, None);
    insert_event(&conn, 20, timestamp(2026, 3, 20, 12, 0), 10_000);

    let snapshot = compute(&conn, StatsPeriod::Year(2026), NOW_2026_07_19, &Utc).unwrap();

    assert_eq!(snapshot.genres.segments.len(), 6);
    assert_eq!(snapshot.genres.segments.last().unwrap().label, "Other");
    assert_eq!(snapshot.genres.denominator_ms, 2_800);
    let share_sum = snapshot
        .genres
        .segments
        .iter()
        .map(|segment| segment.share_percent)
        .sum::<i64>();
    assert!((99..=101).contains(&share_sum));
    assert!(snapshot
        .genres
        .segments
        .iter()
        .all(|segment| !segment.label.is_empty()));
}

#[test]
fn stats_4_highlights_streak_and_discovered() {
    let conn = migrated_conn();
    insert_track(&conn, 1, "Known", "Artist", "", "Rock", 60_000, 0, None);
    insert_track(&conn, 2, "New", "Artist", "", "Rock", 60_000, 0, None);
    insert_event(&conn, 1, timestamp(2025, 12, 1, 12, 0), 30_000);
    insert_event(&conn, 1, timestamp(2026, 1, 1, 23, 30), 30_000);
    insert_event(&conn, 2, timestamp(2026, 1, 2, 23, 30), 30_000);
    let zone = FixedOffset::east_opt(3_600).unwrap();

    let snapshot = compute(&conn, StatsPeriod::Year(2026), NOW_2026_07_19, &zone).unwrap();

    assert_eq!(snapshot.highlights.streak_days, 2);
    assert_eq!(snapshot.highlights.discovered_tracks, 1);
}

#[test]
fn stats_5_top_tracks_sort_toggle_orders_by_time() {
    let conn = migrated_conn();
    insert_track(&conn, 1, "Frequent", "Artist", "", "Rock", 10_000, 0, None);
    insert_track(&conn, 2, "Long", "Artist", "", "Rock", 1_000_000, 0, None);
    for minute in 0..10 {
        insert_event(&conn, 1, timestamp(2026, 4, 1, 12, minute), 10_000);
    }
    insert_event(&conn, 2, timestamp(2026, 4, 2, 12, 0), 1_000_000);

    let snapshot = compute(&conn, StatsPeriod::Year(2026), NOW_2026_07_19, &Utc).unwrap();

    assert_eq!(
        snapshot.top_tracks_sorted(SortBy::Plays)[0].title,
        "Frequent"
    );
    assert_eq!(snapshot.top_tracks_sorted(SortBy::Time)[0].title, "Long");
}

#[test]
fn stats_streak_survives_dst_change() {
    let conn = migrated_conn();
    insert_track(&conn, 1, "DST", "Artist", "", "Rock", 60_000, 0, None);
    for played_at in [
        1_774_567_800,
        1_774_654_200,
        1_774_740_600,
        1_774_823_400,
        1_774_909_800,
    ] {
        insert_event(&conn, 1, played_at, 30_000);
    }

    let dst = compute(
        &conn,
        StatsPeriod::Year(2026),
        timestamp(2026, 4, 1, 0, 0),
        &DstZone,
    )
    .unwrap();
    let utc = compute(
        &conn,
        StatsPeriod::Year(2026),
        timestamp(2026, 4, 1, 0, 0),
        &Utc,
    )
    .unwrap();

    assert_eq!(dst.highlights.streak_days, 5);
    assert_eq!(utc.highlights.streak_days, 5);
}

#[test]
fn compute_is_pure_and_repeatable() {
    let conn = migrated_conn();
    insert_track(
        &conn,
        1,
        "Repeatable",
        "Artist",
        "",
        "Rock",
        60_000,
        0,
        None,
    );
    insert_event(&conn, 1, timestamp(2026, 5, 1, 12, 0), 30_000);

    let first = compute(&conn, StatsPeriod::AllTime, NOW_2026_07_19, &Utc).unwrap();
    let second = compute(&conn, StatsPeriod::AllTime, NOW_2026_07_19, &Utc).unwrap();

    assert_eq!(first, second);
}

#[test]
fn stats_9_group_key_dedups_top_artists_and_genres() {
    let conn = migrated_conn();
    let rows = [
        (1, "Lorna Shore", "Deathcore", 2),
        (2, "lorna shore", "deathcore", 1),
        (3, "Lorna Shore ", "Death core", 1),
    ];
    for (id, artist, genre, plays) in rows {
        insert_track(
            &conn,
            id,
            &format!("Track {id}"),
            artist,
            "",
            genre,
            100_000,
            0,
            None,
        );
        for play in 0..plays {
            insert_event(&conn, id, timestamp(2026, 6, id as u32, 12, play), 100_000);
        }
    }

    let snapshot = compute(&conn, StatsPeriod::Year(2026), NOW_2026_07_19, &Utc).unwrap();

    assert_eq!(snapshot.top_artists.len(), 1);
    assert_eq!(snapshot.top_artists[0].group.label, "Lorna Shore");
    assert_eq!(snapshot.top_artists[0].group.plays, 4);
    assert_eq!(snapshot.top_artists[0].group.ms, 400_000);
    assert_eq!(snapshot.top_artists[0].group.variant_count, 3);
    assert_eq!(
        snapshot
            .genres
            .segments
            .iter()
            .map(|segment| segment.label.as_str())
            .collect::<Vec<_>>(),
        ["Deathcore", "Death core"]
    );
    assert_eq!(
        group_track_ids(&conn, GroupKind::Artist, &snapshot.top_artists[0].group.key,).unwrap(),
        vec![1, 2, 3]
    );
}

#[test]
fn dedup_does_not_mutate_tags() {
    let conn = migrated_conn();
    insert_track(
        &conn,
        1,
        "Read only",
        "Björk",
        "Album Artist",
        "Art Pop",
        60_000,
        0,
        Some("mbid-1"),
    );
    insert_event(&conn, 1, timestamp(2026, 7, 1, 12, 0), 30_000);
    let before = tag_rows(&conn);

    for period in [
        StatsPeriod::YearToDate(2026),
        StatsPeriod::Year(2026),
        StatsPeriod::Last30Days,
        StatsPeriod::AllTime,
    ] {
        let changes = conn.total_changes();
        compute(&conn, period, NOW_2026_07_19, &Utc).unwrap();
        assert_eq!(conn.total_changes(), changes);
        assert_eq!(tag_rows(&conn), before);
    }
}

fn migrated_conn() -> Connection {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn
}

#[allow(clippy::too_many_arguments)]
fn insert_track(
    conn: &Connection,
    id: i64,
    title: &str,
    artist: &str,
    album_artist: &str,
    genre: &str,
    duration_ms: i64,
    play_count: i64,
    artist_mbid: Option<&str>,
) {
    conn.execute(
        "INSERT INTO tracks \
         (id, path, title, artist, album, album_artist, genre, duration_ms, \
          play_count, added_at, artist_mbid) \
         VALUES (?1, ?2, ?3, ?4, 'Album', ?5, ?6, ?7, ?8, 0, ?9)",
        params![
            id,
            format!("/music/{id}.flac"),
            title,
            artist,
            album_artist,
            genre,
            duration_ms,
            play_count,
            artist_mbid,
        ],
    )
    .unwrap();
}

fn insert_event(conn: &Connection, track_id: i64, played_at: i64, ms_played: i64) {
    conn.execute(
        "INSERT INTO listen_events (track_id, played_at, ms_played) VALUES (?1, ?2, ?3)",
        params![track_id, played_at, ms_played],
    )
    .unwrap();
}

fn tag_rows(conn: &Connection) -> Vec<(String, String, String, Option<String>)> {
    conn.prepare("SELECT artist, album_artist, genre, artist_mbid FROM tracks ORDER BY id")
        .unwrap()
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
}

fn timestamp(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> i64 {
    Utc.with_ymd_and_hms(year, month, day, hour, minute, 0)
        .single()
        .unwrap()
        .timestamp()
}

/// Test-only European spring-forward transition without `chrono-tz`.
#[derive(Clone, Copy)]
struct DstZone;

impl DstZone {
    const SWITCH_UNIX: i64 = 1_774_746_000;

    fn offset_at(utc: &NaiveDateTime) -> FixedOffset {
        let seconds = if utc.and_utc().timestamp() < Self::SWITCH_UNIX {
            3_600
        } else {
            7_200
        };
        FixedOffset::east_opt(seconds).expect("valid fixed offset")
    }
}

impl TimeZone for DstZone {
    type Offset = FixedOffset;

    fn from_offset(_: &FixedOffset) -> Self {
        DstZone
    }

    fn offset_from_local_date(&self, date: &NaiveDate) -> MappedLocalTime<FixedOffset> {
        MappedLocalTime::Single(Self::offset_at(&date.and_hms_opt(0, 0, 0).unwrap()))
    }

    fn offset_from_local_datetime(&self, datetime: &NaiveDateTime) -> MappedLocalTime<FixedOffset> {
        MappedLocalTime::Single(Self::offset_at(datetime))
    }

    fn offset_from_utc_date(&self, date: &NaiveDate) -> FixedOffset {
        Self::offset_at(&date.and_hms_opt(0, 0, 0).unwrap())
    }

    fn offset_from_utc_datetime(&self, datetime: &NaiveDateTime) -> FixedOffset {
        Self::offset_at(datetime)
    }
}
