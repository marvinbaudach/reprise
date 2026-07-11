//! Write helpers for per-track listening statistics: star ratings and
//! play-count tracking. Kept separate from `scanner.rs` (which owns the
//! read/write path for library *metadata* scanned off disk) because these
//! writes originate from user interaction and playback events on the UI
//! thread, not from a scan worker.
//!
//! `should_count_play` is a pure predicate — no `Connection`, no I/O — so the
//! "was this track actually listened to" decision is unit-testable on its
//! own, following the same pattern as `track_list::empty_state_for` and
//! `player_bar::should_apply_position_tick`.

use rusqlite::Connection;

/// Ratings are stored as a plain `i32` column with no `CHECK` constraint
/// (see `db.rs`'s schema); clamping here is the single place that keeps
/// out-of-range values (a bad click index, corrupt data) from ever reaching
/// the database.
const RATING_MIN: i32 = 0;
const RATING_MAX: i32 = 5;

/// Sets `track_id`'s rating, clamped to `RATING_MIN..=RATING_MAX`.
pub fn set_rating(conn: &Connection, track_id: i64, rating: i32) -> Result<(), rusqlite::Error> {
    let clamped = rating.clamp(RATING_MIN, RATING_MAX);
    conn.execute(
        "UPDATE tracks SET rating = ?1 WHERE id = ?2",
        rusqlite::params![clamped, track_id],
    )?;
    Ok(())
}

/// Increments `track_id`'s play count and sets `last_played_at` to
/// `now_unix` (seconds since the Unix epoch — the same unit `scanner.rs`
/// uses for `added_at`/`occurred_at`).
pub fn record_play(conn: &Connection, track_id: i64, now_unix: i64) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE tracks SET play_count = play_count + 1, last_played_at = ?1 WHERE id = ?2",
        rusqlite::params![now_unix, track_id],
    )?;
    Ok(())
}

/// Pure "was this track listened to enough to count as a play" predicate:
/// true when the track has a positive duration and the furthest position
/// reached covers at least half of it. `max_position_ms` is the *highest*
/// position observed during playback (see `player_controller.rs`), not the
/// final one — a listener who seeks backward near the end must not lose
/// credit for having already passed the halfway point.
pub fn should_count_play(max_position_ms: i64, duration_ms: i64) -> bool {
    duration_ms > 0 && max_position_ms * 2 >= duration_ms
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded_conn() -> Connection {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) VALUES (1, '/x/a.flac', 'A', 'B', 0)",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn should_count_play_false_at_zero_position() {
        assert!(!should_count_play(0, 1000));
    }

    #[test]
    fn should_count_play_true_at_exactly_half() {
        assert!(should_count_play(500, 1000));
    }

    #[test]
    fn should_count_play_false_just_under_half() {
        assert!(!should_count_play(499, 1000));
    }

    #[test]
    fn should_count_play_false_when_duration_is_zero() {
        assert!(!should_count_play(700, 0));
    }

    #[test]
    fn set_rating_persists_in_range_value() {
        let conn = seeded_conn();
        set_rating(&conn, 1, 3).unwrap();
        let rating: i32 = conn
            .query_row("SELECT rating FROM tracks WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rating, 3);
    }

    #[test]
    fn set_rating_clamps_above_max() {
        let conn = seeded_conn();
        set_rating(&conn, 1, 7).unwrap();
        let rating: i32 = conn
            .query_row("SELECT rating FROM tracks WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rating, 5);
    }

    #[test]
    fn set_rating_clamps_below_min() {
        let conn = seeded_conn();
        set_rating(&conn, 1, -1).unwrap();
        let rating: i32 = conn
            .query_row("SELECT rating FROM tracks WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rating, 0);
    }

    #[test]
    fn record_play_increments_count_and_sets_last_played_at() {
        let conn = seeded_conn();
        record_play(&conn, 1, 1_700_000_000).unwrap();
        let (count, last_played): (i64, i64) = conn
            .query_row(
                "SELECT play_count, last_played_at FROM tracks WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(last_played, 1_700_000_000);

        record_play(&conn, 1, 1_700_000_100).unwrap();
        let (count, last_played): (i64, i64) = conn
            .query_row(
                "SELECT play_count, last_played_at FROM tracks WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, 2);
        assert_eq!(last_played, 1_700_000_100);
    }
}
