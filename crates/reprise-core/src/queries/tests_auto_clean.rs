//! Test coverage for Task 2.3's auto-clean cleanup (`auto_clean_eligible`/
//! `run_auto_clean`) — split into its own file rather than folded into
//! `tests_issues.rs` or `tests_maintenance.rs` (both already close to the
//! project's 800-line rule), purely for size; the two functions themselves
//! live in `issues.rs` — see that file's "Auto-clean" section header
//! comment for the full design rationale this suite verifies.

use super::*;
use crate::library::settings::{self, AutoCleanSetting};
use crate::models::MissingReason;

const DAY: i64 = 86_400;

/// Inserts one track row with an explicit `missing_reason`/`missing_since` —
/// the two columns this suite's deadline math and reason filtering care
/// about. Mirrors `tests_issues.rs`'s own `seed_missing_track`, narrowed to
/// just these fields.
fn seed_missing(conn: &Connection, id: i64, reason: MissingReason, missing_since: i64) {
    conn.execute(
        "INSERT INTO tracks (id, path, title, artist, added_at, missing_since, missing_reason) \
         VALUES (?1, ?2, ?3, '', 0, ?4, ?5)",
        rusqlite::params![
            id,
            format!("/music/{id}.flac"),
            format!("Track {id}"),
            missing_since,
            reason.as_str(),
        ],
    )
    .unwrap();
}

/// Fristmathematik: the deadline is `max(missing_since, armed_at) +
/// days*86400 <= now` — the LATER of the two anchors, not `missing_since`
/// alone. Covers both directions (arming after a long-missing file; going
/// missing after arming) and the exact `<=` boundary.
#[test]
fn auto_clean_eligible_uses_the_later_of_missing_since_and_armed_at() {
    let conn = crate::db::open_migrated(None).unwrap();
    settings::set_missing_auto_clean(&conn, AutoCleanSetting::Days(30)).unwrap();

    // Track 1 went missing long before the feature was armed — arming, not
    // the ancient missing_since, must anchor its deadline (else turning the
    // setting on over a backlog would delete it instantly).
    seed_missing(&conn, 1, MissingReason::Deleted, 0);
    settings::set_auto_clean_armed_at(&conn, 1_000).unwrap();

    let armed_deadline = 1_000 + 30 * DAY;
    assert_eq!(
        auto_clean_eligible(&conn, armed_deadline - 1).unwrap(),
        Vec::<i64>::new(),
        "one second before the deadline: not yet eligible"
    );
    assert_eq!(
        auto_clean_eligible(&conn, armed_deadline).unwrap(),
        vec![1],
        "exactly at the deadline: eligible (<=, not strictly <)"
    );

    // Track 2 goes missing well after arming — its own (later) missing_since
    // now anchors its deadline instead of the earlier armed_at.
    seed_missing(&conn, 2, MissingReason::Deleted, 5_000);
    let missing_since_deadline = 5_000 + 30 * DAY;
    assert_eq!(
        auto_clean_eligible(&conn, missing_since_deadline - 1).unwrap(),
        vec![1],
        "track 2's own later missing_since keeps it ineligible a bit longer"
    );
    assert_eq!(
        auto_clean_eligible(&conn, missing_since_deadline).unwrap(),
        vec![1, 2]
    );
}

/// `unmounted`/`unknown` rows must never be swept up by auto-clean, no
/// matter how long they've sat missing — only `deleted` is provable.
#[test]
fn auto_clean_eligible_never_includes_unmounted_or_unknown() {
    let conn = crate::db::open_migrated(None).unwrap();
    settings::set_missing_auto_clean(&conn, AutoCleanSetting::Days(30)).unwrap();
    settings::set_auto_clean_armed_at(&conn, 0).unwrap();

    seed_missing(&conn, 1, MissingReason::Unmounted, 0);
    seed_missing(&conn, 2, MissingReason::Unknown, 0);
    seed_missing(&conn, 3, MissingReason::Deleted, 0);

    let far_future = 1_000 * DAY;
    assert_eq!(
        auto_clean_eligible(&conn, far_future).unwrap(),
        vec![3],
        "only the deleted row is ever eligible, regardless of the others' age"
    );
}

/// A missing (default) or explicitly `Off` `missing_auto_clean` setting must
/// never make any row eligible, even long past what would otherwise be a
/// deadline.
#[test]
fn auto_clean_eligible_is_empty_when_the_setting_is_off() {
    let conn = crate::db::open_migrated(None).unwrap();
    settings::set_auto_clean_armed_at(&conn, 0).unwrap();
    seed_missing(&conn, 1, MissingReason::Deleted, 0);
    let far_future = 1_000 * DAY;

    // Never written: the documented Off default.
    assert_eq!(
        auto_clean_eligible(&conn, far_future).unwrap(),
        Vec::<i64>::new()
    );

    // Explicitly Off behaves identically to never-written.
    settings::set_missing_auto_clean(&conn, AutoCleanSetting::Off).unwrap();
    assert_eq!(
        auto_clean_eligible(&conn, far_future).unwrap(),
        Vec::<i64>::new()
    );
}

/// A duration alone, with no arming date, must never run — the fail-safe
/// direction: "did nothing" is recoverable, "deleted N tracks" is not.
#[test]
fn auto_clean_eligible_is_empty_without_an_armed_at() {
    let conn = crate::db::open_migrated(None).unwrap();
    settings::set_missing_auto_clean(&conn, AutoCleanSetting::Days(30)).unwrap();
    // auto_clean_armed_at deliberately never written.
    seed_missing(&conn, 1, MissingReason::Deleted, 0);
    assert_eq!(
        auto_clean_eligible(&conn, 1_000 * DAY).unwrap(),
        Vec::<i64>::new(),
        "a duration alone, with no arming date, must never run"
    );
}

/// `run_auto_clean` hard-deletes exactly the eligible ids — a real,
/// cascading delete with no tombstone and no undo — and leaves every other
/// row (wrong reason, or not yet past its deadline) untouched.
#[test]
fn run_auto_clean_hard_deletes_eligible_tracks_and_spares_the_rest() {
    let mut conn = crate::db::open_migrated(None).unwrap();
    settings::set_missing_auto_clean(&conn, AutoCleanSetting::Days(30)).unwrap();
    settings::set_auto_clean_armed_at(&conn, 0).unwrap();

    seed_missing(&conn, 1, MissingReason::Deleted, 0); // past its deadline
    seed_missing(&conn, 2, MissingReason::Unmounted, 0); // never eligible
    seed_missing(&conn, 3, MissingReason::Deleted, 900 * DAY); // too recent
    conn.execute(
        "INSERT INTO listen_events (track_id, played_at, ms_played) VALUES (1, 0, 1000)",
        [],
    )
    .unwrap();

    let now = 30 * DAY;
    let removed = run_auto_clean(&mut conn, now).unwrap();
    assert_eq!(removed, vec![1]);

    let remaining_ids: Vec<i64> = conn
        .prepare("SELECT id FROM tracks ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        remaining_ids,
        vec![2, 3],
        "only the eligible row is hard-deleted; the rest survive untouched"
    );

    let listen_event_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM listen_events WHERE track_id = 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        listen_event_count, 0,
        "the cascade wipes listening history — no tombstone, no undo"
    );
}

/// Finding 1 (Important, review pass): proves `run_auto_clean`'s guard
/// against a resurrection that lands mid-delete, not just before it — the
/// TOCTOU race `maintenance::remove_auto_clean_eligible_tracks`'s doc
/// comment describes. `auto_clean_eligible`'s own `SELECT` and the per-id
/// `DELETE` it feeds are not one atomic transaction, so the scanner/watcher
/// (its own OS thread, its own `rusqlite::Connection`, a genuine concurrent
/// writer under this database's WAL mode — not a hypothetical) can commit a
/// resurrection AFTER an id is captured in the eligible-ids snapshot but
/// BEFORE that id's `DELETE` runs. A real thread race can't be scheduled
/// deterministically in a unit test, so this proves the guard directly
/// instead — the exact shape `tests_issues.rs`'s `purge_tombstones_
/// survives_a_resurrection_racing_the_delete_itself` uses for the sibling
/// race on the tombstone-purge path: make rows eligible, then (simulating
/// what the watcher would have committed in the window) resurrect one via
/// direct SQL — clear its `missing_since`/`missing_reason` — then call the
/// delete path with the STALE id list that still contains it, and assert
/// that row survives with its playlist membership and listen history
/// intact while the genuinely-eligible ids are deleted.
///
/// Before this fix (`run_auto_clean` routed through the unguarded
/// `maintenance::remove_tracks`, a bare `DELETE FROM tracks WHERE id = ?1`
/// with no re-check), this exact call would have hard-deleted the
/// resurrected row and cascaded away its playlist membership and listen
/// history right along with it; this test fails against that code and
/// passes against the `RemoveGuard::AutoCleanEligible`-guarded path.
#[test]
fn run_auto_clean_survives_a_resurrection_racing_the_delete_itself() {
    let mut conn = crate::db::open_migrated(None).unwrap();
    settings::set_missing_auto_clean(&conn, AutoCleanSetting::Days(30)).unwrap();
    settings::set_auto_clean_armed_at(&conn, 0).unwrap();

    for id in 1..=3 {
        seed_missing(&conn, id, MissingReason::Deleted, 0);
    }
    let playlist_id = crate::library::playlists::create(&conn, "Race").unwrap();
    crate::library::playlists::add_tracks(&mut conn, playlist_id, &[1, 2, 3]).unwrap();
    conn.execute(
        "INSERT INTO listen_events (track_id, played_at, ms_played) VALUES (2, 1000, 5000)",
        [],
    )
    .unwrap();

    let now = 30 * DAY;
    let eligible = auto_clean_eligible(&conn, now).unwrap();
    assert_eq!(
        eligible,
        vec![1, 2, 3],
        "all three rows are past their deadline at selection time"
    );

    // Simulate the watcher committing a resurrection of id 2 in the window
    // between `auto_clean_eligible`'s SELECT (above, already captured) and
    // the DELETE reaching that id — the file reappeared, so this row is
    // legitimately live again.
    conn.execute(
        "UPDATE tracks SET missing_since = NULL, missing_reason = NULL WHERE id = 2",
        [],
    )
    .unwrap();
    let stale_snapshot = eligible;

    let deleted =
        remove_tracks_impl(&mut conn, &stale_snapshot, RemoveGuard::AutoCleanEligible).unwrap();

    assert_eq!(
        deleted,
        vec![1, 3],
        "only the ids still eligible at DELETE time may be removed"
    );

    let track_count: i64 = conn
        .query_row("SELECT count(*) FROM tracks WHERE id = 2", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        track_count, 1,
        "the mid-delete-resurrected row must survive, not be hard-deleted"
    );

    let playlist_rows: Vec<i64> = conn
        .prepare("SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position")
        .unwrap()
        .query_map([playlist_id], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        playlist_rows,
        vec![2],
        "the survivor's playlist membership must not be cascaded away"
    );

    let listen_event_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM listen_events WHERE track_id = 2",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        listen_event_count, 1,
        "the survivor's listening history must not be cascaded away"
    );
}
