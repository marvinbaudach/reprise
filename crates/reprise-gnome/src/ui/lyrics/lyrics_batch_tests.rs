use std::rc::Rc;
use std::sync::atomic::Ordering;

use super::*;

#[test]
fn cover_completion_starts_lyrics_only_after_the_subscription_is_armed() {
    assert!(!cover_batch_finished(false, CoverBatchState::Complete));
    assert!(!cover_batch_finished(true, CoverBatchState::Running));
    assert!(cover_batch_finished(true, CoverBatchState::Idle));
    assert!(cover_batch_finished(true, CoverBatchState::Complete));
    assert!(cover_batch_finished(true, CoverBatchState::Failed));
}

#[test]
fn net_1a_the_batch_gate_follows_the_global_online_sources_switch() {
    let conn = Rc::new(crate::test_db::open().unwrap());
    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::ONLINE_LYRICS_MODULE, true)
        .unwrap();
    let batch = LyricsBatch::new(&conn);
    assert!(batch.enabled.load(Ordering::Relaxed));

    reprise_core::online_sources::set_enabled(&conn, false).unwrap();
    batch.republish_enabled();
    assert!(!batch.enabled.load(Ordering::Relaxed));

    reprise_core::online_sources::set_enabled(&conn, true).unwrap();
    batch.republish_enabled();
    assert!(batch.enabled.load(Ordering::Relaxed));
}

#[test]
fn a_recent_clean_restart_skips_the_automatic_batch_without_showing_progress() {
    let conn = Rc::new(crate::test_db::open().unwrap());
    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::ONLINE_LYRICS_MODULE, true)
        .unwrap();
    let batch = LyricsBatch::new(&conn);
    let mut previous_session = reprise_core::library::session::SessionState::default();
    reprise_core::library::session::mark_clean_exit_now(&mut previous_session, "/music".into());

    batch.start_automatically(&previous_session, "/music");

    assert_eq!(batch.generation.load(Ordering::Relaxed), 0);
    assert_eq!(batch.progress.get().state, LyricsBatchState::Idle);

    batch.start();

    assert_eq!(batch.generation.load(Ordering::Relaxed), 1);
}

#[test]
fn a_switched_off_module_never_reaches_the_due_check() {
    let consulted = std::cell::Cell::new(false);

    let starts = automatic_start_decision(false, || {
        consulted.set(true);
        true
    });

    assert!(!starts);
    assert!(
        !consulted.get(),
        "the due-check logs the reason it skipped; a module that is off must not \
         produce a clean-exit reason for work it would never have done"
    );
}

#[test]
fn an_enabled_module_still_obeys_the_due_check() {
    assert!(automatic_start_decision(true, || true));
    assert!(!automatic_start_decision(true, || false));
}

#[test]
fn a_hard_kill_keeps_the_automatic_batch_due() {
    let conn = Rc::new(crate::test_db::open().unwrap());
    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::ONLINE_LYRICS_MODULE, true)
        .unwrap();
    let batch = LyricsBatch::new(&conn);

    batch.start_automatically(
        &reprise_core::library::session::SessionState::default(),
        "/music",
    );

    assert_eq!(batch.generation.load(Ordering::Relaxed), 1);
}

#[test]
fn a_dead_progress_subscriber_stops_being_called_and_is_pruned() {
    let conn = Rc::new(crate::test_db::open().unwrap());
    let batch = LyricsBatch::new(&conn);
    let alive = Rc::new(std::cell::Cell::new(true));
    let calls = Rc::new(std::cell::Cell::new(0));
    let alive_for_probe = alive.clone();
    let calls_for_callback = calls.clone();
    batch.subscribe_progress(
        move || alive_for_probe.get(),
        move |_| calls_for_callback.set(calls_for_callback.get() + 1),
    );

    alive.set(false);
    batch.set_progress_for_test(LyricsBatchProgress::running(2));

    assert_eq!(calls.get(), 1);
    assert_eq!(batch.subscribers.len(), 0);
}
