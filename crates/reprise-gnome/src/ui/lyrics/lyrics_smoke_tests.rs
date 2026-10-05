use reprise_core::library::startup_tasks::{
    lyrics_last_full_sweep, lyrics_scope, lyrics_watermark, now_unix,
};
use reprise_core::modules::{self, ONLINE_LYRICS_MODULE};
use reprise_core::online_sources;

use super::*;

#[test]
fn the_isolated_gate_allows_the_lyrics_module() {
    let db = crate::test_db::open_fresh().unwrap();
    assert_eq!(
        online_sources::network_allowed(&db, &ONLINE_LYRICS_MODULE).ok(),
        Some(false)
    );

    open_isolated_lyrics_gate(&db).unwrap();

    assert_eq!(
        online_sources::network_allowed(&db, &ONLINE_LYRICS_MODULE).ok(),
        Some(true)
    );
    assert_eq!(online_sources::is_enabled(&db).ok(), Some(true));
    assert_eq!(
        modules::is_enabled(&db, &ONLINE_LYRICS_MODULE).ok(),
        Some(true)
    );
}

#[test]
fn the_module_alone_does_not_allow_the_network() {
    let db = crate::test_db::open_fresh().unwrap();
    modules::set_enabled(&db, &ONLINE_LYRICS_MODULE, true).unwrap();

    assert_eq!(
        modules::is_enabled(&db, &ONLINE_LYRICS_MODULE).ok(),
        Some(true)
    );
    assert_eq!(
        online_sources::network_allowed(&db, &ONLINE_LYRICS_MODULE).ok(),
        Some(false)
    );
}

#[test]
fn a_settled_startup_sweep_covers_only_tracks_added_afterwards() {
    let db = crate::test_db::open_fresh().unwrap();
    let before = lyrics_scope(
        lyrics_watermark(&db),
        lyrics_last_full_sweep(&db),
        now_unix(),
    );
    assert_eq!(before, LyricsScope::Everything);

    settle_startup_lyrics_sweep(&db);

    let watermark = lyrics_watermark(&db).expect("the settled pass records a watermark");
    let after = lyrics_scope(Some(watermark), lyrics_last_full_sweep(&db), now_unix());
    assert_eq!(after, LyricsScope::AddedSince(watermark));
}

#[test]
fn a_gate_write_that_lost_a_lock_race_is_tried_again() {
    let mut failures_left = 2;
    let result = retry_while_locked(|| {
        if failures_left == 0 {
            return Ok(());
        }
        failures_left -= 1;
        Err("database is locked")
    });

    assert_eq!(result, Ok(()));
    assert_eq!(failures_left, 0);
}

#[test]
fn a_gate_write_that_never_succeeds_reports_its_last_error() {
    let mut attempts = 0;
    let result = retry_while_locked(|| {
        attempts += 1;
        Err(attempts)
    });

    assert_eq!(result, Err(GATE_ATTEMPTS));
}
