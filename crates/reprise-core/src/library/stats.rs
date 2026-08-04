//! Write helpers for per-track listening statistics: star ratings and
//! play-count tracking. Kept separate from `scanner.rs` (which owns the
//! read/write path for library *metadata* scanned off disk) by *what* they
//! write rather than by where from: one already-known row at a time, addressed
//! by id, recording what a listener did rather than what a file says.
//!
//! Which thread calls them is no longer one answer, and used to be documented
//! as if it were. The desktop still rates and counts plays from its UI thread.
//! Android does neither: play counts go to a writer thread the playback session
//! owns (`reprise-android-ffi`'s `play_recorder`), and a star tap goes to a
//! second one the activity owns, precisely so that Media3's application thread
//! and the main thread never wait on SQLite. Tag writeback calls
//! `set_rating_in` from inside a transaction it is already holding.
//!
//! So everything here has to be callable from any thread holding its own
//! [`Db`] handle or `Connection`, and losing to another writer is an ordinary
//! outcome rather than a defect — a scan wraps a whole folder walk in one
//! transaction, which is what [`is_database_busy`] exists to let a caller
//! recognise.
//!
//! Android's process-surviving play journal adds one more constraint without
//! moving its queue into Core: [`record_journaled_play`] joins the ordinary
//! count statement and the applied sequence high-water in one transaction.
//! Core owns those two SQL statements; the journal file and its lifecycle stay
//! at the Android boundary.
//!
//! `should_count_play` is a pure predicate — no `Connection`, no I/O — so the
//! "was this track actually listened to" decision is unit-testable on its
//! own, following the same pattern as `track_list::empty_state_for` and
//! `player_bar::should_apply_position_tick`.

use rusqlite::Connection;
use std::path::Path;

use crate::db::Db;

/// Ratings are stored as a plain `i32` column with no `CHECK` constraint
/// (see `db.rs`'s schema); clamping here is the single place that keeps
/// out-of-range values (a bad click index, corrupt data) from ever reaching
/// the database.
const RATING_MIN: i32 = 0;
const RATING_MAX: i32 = 5;

/// Sets `track_id`'s rating, clamped to `RATING_MIN..=RATING_MAX`.
///
/// Addresses the row by id alone, exactly as it always has: a row whose
/// `removed_at` is set is still rated. Its desktop caller only ever rates rows
/// that came out of a query already filtering `removed_at IS NULL`, so it
/// cannot tell the difference today — which is precisely why the condition must
/// not be added here on the way past. A boundary that has to *report* whether a
/// live row matched asks [`set_rating_if_present`] instead.
pub fn set_rating(db: &Db, track_id: i64, rating: i32) -> Result<(), rusqlite::Error> {
    set_rating_in(db.conn(), track_id, rating)
}

/// Sets a rating on a row that is still part of the library, and reports
/// whether one matched.
///
/// The stricter answer exists for callers that show the user a success or a
/// failure — the Android sheet's stars, which have no other way to learn that
/// the row behind them has since been removed. [`set_rating`] keeps the silent
/// zero-row behaviour its desktop call sites were written against.
pub fn set_rating_if_present(db: &Db, track_id: i64, rating: i32) -> Result<bool, rusqlite::Error> {
    let clamped = rating.clamp(RATING_MIN, RATING_MAX);
    let changed = db.conn().execute(
        "UPDATE tracks SET rating = ?1 WHERE id = ?2 AND removed_at IS NULL",
        rusqlite::params![clamped, track_id],
    )?;
    Ok(changed == 1)
}

/// The unconditional rating write, on a bare `Connection`.
///
/// This is the one statement behind [`set_rating`]; tag editing needs it on a
/// connection it is already holding a transaction on, which a `Db` handle
/// cannot express.
pub(crate) fn set_rating_in(
    conn: &Connection,
    track_id: i64,
    rating: i32,
) -> Result<(), rusqlite::Error> {
    let clamped = rating.clamp(RATING_MIN, RATING_MAX);
    conn.execute(
        "UPDATE tracks SET rating = ?1 WHERE id = ?2",
        rusqlite::params![clamped, track_id],
    )?;
    Ok(())
}

/// Sets a rating only if the row is still live *and* still describes the file
/// on disk that the writeback just tagged.
///
/// The extra `path` predicate is the point: between reading a file and writing
/// its tags back, the row may have been re-pointed at another file, and rating
/// that one would attribute the edit to the wrong track.
pub(crate) fn set_rating_for_registered_track(
    conn: &Connection,
    track_id: i64,
    path: &Path,
    rating: i32,
) -> Result<bool, rusqlite::Error> {
    let clamped = rating.clamp(RATING_MIN, RATING_MAX);
    let changed = conn.execute(
        "UPDATE tracks SET rating=?1 \
         WHERE id=?2 AND path=?3 AND removed_at IS NULL",
        rusqlite::params![clamped, track_id, path.to_string_lossy()],
    )?;
    Ok(changed == 1)
}

/// Increments `track_id`'s play count and sets `last_played_at` to
/// `now_unix` (seconds since the Unix epoch — the same unit `scanner.rs`
/// uses for `added_at`/`occurred_at`).
pub fn record_play(db: &Db, track_id: i64, now_unix: i64) -> Result<(), rusqlite::Error> {
    record_play_in(db.conn(), track_id, now_unix)
}

/// The play-count update on a caller-owned connection or transaction.
///
/// The Android journal pairs this statement with its applied high-water mark
/// in one transaction. Keeping the statement here avoids a second transaction
/// and keeps SQL ownership out of the frontend crate.
pub(crate) fn record_play_in(
    conn: &Connection,
    track_id: i64,
    now_unix: i64,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE tracks SET play_count = play_count + 1, last_played_at = ?1 WHERE id = ?2",
        rusqlite::params![now_unix, track_id],
    )?;
    Ok(())
}

/// Returns the highest Android play-journal sequence committed to this
/// library. Journal sequences start at one, so zero means none has landed.
pub fn play_journal_high_water(db: &Db) -> Result<i64, rusqlite::Error> {
    db.conn().query_row(
        "SELECT applied_sequence FROM android_play_count_journal_state WHERE singleton = 1",
        [],
        |row| row.get(0),
    )
}

/// Applies a journaled play exactly once.
///
/// A sequence at or below the stored high-water mark is an already-committed
/// replay and returns `false`. A newer sequence increments the count and moves
/// the mark in the same transaction, returning `true`. The immediate
/// transaction matters because this is a read-then-write operation racing the
/// scanner's long write transaction: SQLite's ordinary busy timeout must
/// govern the race instead of a deferred snapshot-upgrade failure.
pub fn record_journaled_play(
    db: &Db,
    sequence: i64,
    track_id: i64,
    now_unix: i64,
) -> Result<bool, rusqlite::Error> {
    crate::events::in_txn_immediate(db.conn(), |conn| {
        let high_water: i64 = conn.query_row(
            "SELECT applied_sequence FROM android_play_count_journal_state WHERE singleton = 1",
            [],
            |row| row.get(0),
        )?;
        if sequence <= high_water {
            return Ok(false);
        }
        record_play_in(conn, track_id, now_unix)?;
        conn.execute(
            "UPDATE android_play_count_journal_state SET applied_sequence = ?1 \
             WHERE singleton = 1",
            [sequence],
        )?;
        Ok(true)
    })
}

/// Whether a failed write lost to another writer holding the database, rather
/// than failing for a reason that offering it again cannot fix.
///
/// Exists here because [`Db`] deliberately does not hand out its `Connection`
/// (see `db_handle.rs`): a frontend that has to decide whether a write is worth
/// retrying would otherwise take a `rusqlite` dependency of its own just to
/// read one error code, which is precisely the door that type keeps shut. The
/// Android play recorder is the first caller — its writes race a SAF scan that
/// wraps a whole folder walk in one transaction, so `SQLITE_BUSY` there is an
/// ordinary occurrence rather than a defect.
pub fn is_database_busy(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(failure, _)
            if matches!(
                failure.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            )
    )
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

    fn seeded_conn() -> Db {
        let conn = Db::open_in_memory().unwrap();
        conn.conn()
            .execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) VALUES (1, '/x/a.flac', 'A', 'B', 0)",
            [],
        )
        .unwrap();
        conn
    }

    /// Seeds row 2 as a track the library has since removed, so both rating
    /// writers can be asked what they do with it.
    fn seeded_conn_with_removed_track() -> Db {
        let conn = seeded_conn();
        conn.conn()
            .execute(
                "INSERT INTO tracks (id, path, title, artist, added_at, removed_at) \
                 VALUES (2, '/x/gone.flac', 'Gone', 'B', 0, 100)",
                [],
            )
            .unwrap();
        conn
    }

    fn rating_of(db: &Db, track_id: i64) -> i32 {
        db.conn()
            .query_row("SELECT rating FROM tracks WHERE id = ?1", [track_id], |r| {
                r.get(0)
            })
            .unwrap()
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
            .conn()
            .query_row("SELECT rating FROM tracks WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rating, 3);
    }

    #[test]
    fn set_rating_clamps_above_max() {
        let conn = seeded_conn();
        set_rating(&conn, 1, 7).unwrap();
        let rating: i32 = conn
            .conn()
            .query_row("SELECT rating FROM tracks WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rating, 5);
    }

    #[test]
    fn set_rating_clamps_below_min() {
        let conn = seeded_conn();
        set_rating(&conn, 1, -1).unwrap();
        let rating: i32 = conn
            .conn()
            .query_row("SELECT rating FROM tracks WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rating, 0);
    }

    /// The desktop's rating column addresses a row by id and expects the write
    /// to land. Adding `removed_at IS NULL` here would silently narrow that for
    /// a caller that never asked, so this pins the two writers apart.
    #[test]
    fn set_rating_writes_a_removed_row_while_the_reporting_writer_refuses_it() {
        let conn = seeded_conn_with_removed_track();

        set_rating(&conn, 2, 3).unwrap();
        assert_eq!(rating_of(&conn, 2), 3);

        assert!(!set_rating_if_present(&conn, 2, 5).unwrap());
        assert_eq!(rating_of(&conn, 2), 3);
    }

    #[test]
    fn set_rating_if_present_reports_the_live_row_it_wrote() {
        let conn = seeded_conn_with_removed_track();

        assert!(set_rating_if_present(&conn, 1, 4).unwrap());
        assert_eq!(rating_of(&conn, 1), 4);
        assert!(!set_rating_if_present(&conn, 404, 4).unwrap());
    }

    #[test]
    fn record_play_increments_count_and_sets_last_played_at() {
        let conn = seeded_conn();
        record_play(&conn, 1, 1_700_000_000).unwrap();
        let (count, last_played): (i64, i64) = conn
            .conn()
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
            .conn()
            .query_row(
                "SELECT play_count, last_played_at FROM tracks WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, 2);
        assert_eq!(last_played, 1_700_000_100);
    }

    #[test]
    fn journaled_play_and_its_high_water_commit_exactly_once() {
        let conn = seeded_conn();
        assert_eq!(play_journal_high_water(&conn).unwrap(), 0);

        assert!(record_journaled_play(&conn, 7, 1, 1_700_000_000).unwrap());
        assert_eq!(play_journal_high_water(&conn).unwrap(), 7);
        assert!(
            !record_journaled_play(&conn, 7, 1, 1_700_000_000).unwrap(),
            "replaying the committed entry must be a no-op",
        );
        assert!(
            !record_journaled_play(&conn, 6, 1, 1_699_999_999).unwrap(),
            "an older entry must stay behind the high-water mark",
        );

        let (count, last_played): (i64, i64) = conn
            .conn()
            .query_row(
                "SELECT play_count, last_played_at FROM tracks WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, 1);
        assert_eq!(last_played, 1_700_000_000);
    }

    #[test]
    fn journaled_play_rolls_back_when_its_high_water_cannot_move() {
        let conn = seeded_conn();
        conn.conn()
            .execute_batch(
                "CREATE TRIGGER reject_play_high_water \
                 BEFORE UPDATE ON android_play_count_journal_state \
                 BEGIN SELECT RAISE(ABORT, 'high-water unavailable'); END;",
            )
            .unwrap();

        let error = record_journaled_play(&conn, 1, 1, 1_700_000_000).unwrap_err();

        assert!(error.to_string().contains("high-water unavailable"));
        let count: i64 = conn
            .conn()
            .query_row("SELECT play_count FROM tracks WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "the count and high-water must roll back together");
        assert_eq!(play_journal_high_water(&conn).unwrap(), 0);
    }

    /// The predicate has to recognise the error SQLite really produces under
    /// contention, not the one we imagine it produces — so this test creates
    /// the contention rather than constructing the error value. The contending
    /// connection waits zero milliseconds, which makes the race the test's
    /// instead of the clock's.
    #[test]
    fn a_write_that_lost_to_another_writer_is_told_apart_from_one_worth_no_retry() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("reprise.db");
        let holder = Db::open_migrated(Some(&path)).unwrap();
        holder
            .conn()
            .execute_batch("BEGIN IMMEDIATE; UPDATE tracks SET rating = 1")
            .unwrap();

        let contender = crate::db::open_with_options(Some(&path), 0).unwrap();
        let busy = contender
            .execute("UPDATE tracks SET play_count = play_count + 1", [])
            .unwrap_err();

        assert!(is_database_busy(&busy), "got {busy:?}");
        assert!(!is_database_busy(&rusqlite::Error::QueryReturnedNoRows));
    }
}
