use std::future::Future;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::*;

const LYRICS_WATERMARK_KEY: &str = "startup_tasks.lyrics_watermark";
const LYRICS_FULL_SWEEP_KEY: &str = "startup_tasks.lyrics_full_sweep";

fn controlled_batch(conn: &Rc<Db>) -> (Rc<LyricsBatch>, async_channel::Receiver<WorkerRequest>) {
    reprise_core::modules::set_enabled(conn, &reprise_core::modules::ONLINE_LYRICS_MODULE, true)
        .unwrap();
    let (sender, receiver) = async_channel::unbounded();
    let batch = Rc::new(LyricsBatch {
        conn: conn.clone(),
        worker: LyricsBatchWorker { sender },
        cancellation: ScanCancellation::default(),
        enabled: Arc::new(AtomicBool::new(true)),
        generation: Arc::new(AtomicU64::new(0)),
        running: Cell::new(false),
        progress: Cell::new(LyricsBatchProgress::idle()),
        subscribers: ProgressSubscribers::default(),
    });
    (batch, receiver)
}

fn insert_track(db: &Db, id: i64, path: &str, added_at: i64, file_mtime: i64) {
    crate::test_db::connection(db)
        .execute(
            "INSERT INTO tracks (id, path, title, artist, added_at, file_mtime) \
             VALUES (?1, ?2, '', '', ?3, ?4)",
            rusqlite::params![id, path, added_at, file_mtime],
        )
        .unwrap();
}

fn set_lyrics_timestamps(db: &Db, watermark: i64, last_full_sweep: i64) {
    reprise_core::library::settings::set_setting(db, LYRICS_WATERMARK_KEY, &watermark.to_string())
        .unwrap();
    reprise_core::library::settings::set_setting(
        db,
        LYRICS_FULL_SWEEP_KEY,
        &last_full_sweep.to_string(),
    )
    .unwrap();
}

fn run<T>(future: impl Future<Output = T>) -> T {
    let context = glib::MainContext::new();
    context
        .with_thread_default(|| context.block_on(future))
        .unwrap()
}

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
fn lyr_6_an_automatic_pass_covers_only_tracks_added_since_the_last_completed_one() {
    run(async {
        let conn = Rc::new(crate::test_db::open().unwrap());
        let now = startup_tasks::now_unix();
        set_lyrics_timestamps(&conn, 100, now);
        insert_track(&conn, 1, "/music/old.flac", 100, 100);
        insert_track(&conn, 2, "/music/new.flac", 101, 100);
        let (batch, requests) = controlled_batch(&conn);

        batch.start_automatically(
            &reprise_core::library::session::SessionState::default(),
            "/music",
        );

        assert_eq!(batch.progress.get().total, 1);
        let request = requests.try_recv().unwrap();
        assert_eq!(request.tracks.len(), 1);
        assert_eq!(request.tracks[0].path.to_str(), Some("/music/new.flac"));
        request.events.try_send(WorkerEvent::Cancelled).unwrap();
        glib::timeout_future(Duration::from_millis(1)).await;
    });
}

#[test]
fn lyr_6_a_cancelled_pass_leaves_the_watermark_untouched() {
    run(async {
        let conn = Rc::new(crate::test_db::open().unwrap());
        let now = startup_tasks::now_unix();
        set_lyrics_timestamps(&conn, 100, now);
        insert_track(&conn, 1, "/music/new.flac", 101, 100);
        let (batch, requests) = controlled_batch(&conn);
        batch.start_automatically(
            &reprise_core::library::session::SessionState::default(),
            "/music",
        );
        let request = requests.try_recv().unwrap();

        request.events.try_send(WorkerEvent::Cancelled).unwrap();
        glib::timeout_future(Duration::from_millis(1)).await;

        assert_eq!(startup_tasks::lyrics_watermark(&conn), Some(100));
        assert_eq!(batch.progress.get().state, LyricsBatchState::Idle);
    });
}

#[test]
fn lyr_6_a_completed_pass_advances_the_watermark() {
    run(async {
        let conn = Rc::new(crate::test_db::open().unwrap());
        let now = startup_tasks::now_unix();
        set_lyrics_timestamps(&conn, 100, now);
        insert_track(&conn, 1, "/music/new.flac", 101, 100);
        let (batch, requests) = controlled_batch(&conn);
        batch.start_automatically(
            &reprise_core::library::session::SessionState::default(),
            "/music",
        );
        let request = requests.try_recv().unwrap();

        request
            .events
            .try_send(WorkerEvent::Progress(LyricsBatchProgress::running(0)))
            .unwrap();
        glib::timeout_future(Duration::from_millis(1)).await;

        assert!(startup_tasks::lyrics_watermark(&conn).unwrap() > 100);
    });
}

#[test]
fn lyr_6_a_library_that_never_completed_a_pass_is_swept_in_full() {
    run(async {
        let conn = Rc::new(crate::test_db::open().unwrap());
        insert_track(&conn, 1, "/music/old.flac", 1, 1);
        insert_track(&conn, 2, "/music/new.flac", 2, 2);
        let (batch, requests) = controlled_batch(&conn);

        batch.start_automatically(
            &reprise_core::library::session::SessionState::default(),
            "/music",
        );

        let request = requests.try_recv().unwrap();
        assert_eq!(request.tracks.len(), 2);
        assert!(startup_tasks::lyrics_last_full_sweep(&conn).is_some());
        assert_eq!(startup_tasks::lyrics_watermark(&conn), None);
        request.events.try_send(WorkerEvent::Cancelled).unwrap();
        glib::timeout_future(Duration::from_millis(1)).await;
    });
}

#[test]
fn lyr_6_a_full_sweep_attempt_defers_the_next_one_by_the_full_interval() {
    run(async {
        let conn = Rc::new(crate::test_db::open().unwrap());
        let now = startup_tasks::now_unix();
        set_lyrics_timestamps(
            &conn,
            100,
            now - startup_tasks::LYRICS_FULL_SWEEP_INTERVAL_SECONDS,
        );
        insert_track(&conn, 1, "/music/old.flac", 100, 100);
        insert_track(&conn, 2, "/music/new.flac", 101, 100);
        let (batch, requests) = controlled_batch(&conn);

        batch.start();
        let full_request = requests.try_recv().unwrap();
        assert_eq!(full_request.tracks.len(), 2);
        full_request
            .events
            .try_send(WorkerEvent::Cancelled)
            .unwrap();
        glib::timeout_future(Duration::from_millis(1)).await;

        batch.start_automatically(
            &reprise_core::library::session::SessionState::default(),
            "/music",
        );

        let request = requests.try_recv().unwrap();
        assert_eq!(request.tracks.len(), 1);
        assert_eq!(request.tracks[0].path.to_str(), Some("/music/new.flac"));
        request.events.try_send(WorkerEvent::Cancelled).unwrap();
        glib::timeout_future(Duration::from_millis(1)).await;
    });
}

#[test]
fn lyr_6_a_second_automatic_start_keeps_the_active_pass() {
    run(async {
        let conn = Rc::new(crate::test_db::open().unwrap());
        let now = startup_tasks::now_unix();
        set_lyrics_timestamps(
            &conn,
            100,
            now - startup_tasks::LYRICS_FULL_SWEEP_INTERVAL_SECONDS,
        );
        insert_track(&conn, 1, "/music/old.flac", 100, 100);
        insert_track(&conn, 2, "/music/new.flac", 101, 100);
        let (batch, requests) = controlled_batch(&conn);

        batch.start_automatically(
            &reprise_core::library::session::SessionState::default(),
            "/music",
        );
        let request = requests.try_recv().unwrap();
        let generation = batch.generation_for_test();

        batch.start_automatically(
            &reprise_core::library::session::SessionState::default(),
            "/music",
        );

        assert_eq!(batch.generation_for_test(), generation);
        assert!(requests.is_empty());
        assert!(!cancelled(&request));
        request.events.try_send(WorkerEvent::Cancelled).unwrap();
        glib::timeout_future(Duration::from_millis(1)).await;
    });
}

#[test]
fn lyr_6_an_empty_explicit_pass_releases_the_automatic_start_guard() {
    run(async {
        let conn = Rc::new(crate::test_db::open().unwrap());
        let now = startup_tasks::now_unix();
        set_lyrics_timestamps(&conn, 100, now);
        insert_track(&conn, 1, "/music/present.flac", 101, 100);
        let (batch, requests) = controlled_batch(&conn);

        batch.start_automatically(
            &reprise_core::library::session::SessionState::default(),
            "/music",
        );
        let superseded_request = requests.try_recv().unwrap();

        crate::test_db::connection(&conn)
            .execute("UPDATE tracks SET missing_since = 1 WHERE id = 1", [])
            .unwrap();
        batch.start();
        assert_eq!(batch.progress.get().state, LyricsBatchState::Complete);

        superseded_request
            .events
            .try_send(WorkerEvent::Cancelled)
            .unwrap();
        glib::timeout_future(Duration::from_millis(1)).await;

        let watermark = startup_tasks::lyrics_watermark(&conn).unwrap();
        insert_track(
            &conn,
            2,
            "/music/new-after-empty-pass.flac",
            watermark + 1,
            watermark + 1,
        );
        batch.start_automatically(
            &reprise_core::library::session::SessionState::default(),
            "/music",
        );

        let request = requests.try_recv().unwrap();
        assert_eq!(request.tracks.len(), 1);
        assert_eq!(
            request.tracks[0].path.to_str(),
            Some("/music/new-after-empty-pass.flac")
        );
        request.events.try_send(WorkerEvent::Cancelled).unwrap();
        glib::timeout_future(Duration::from_millis(1)).await;
    });
}

#[test]
fn lyr_6_switching_the_module_on_still_sweeps_the_full_library() {
    run(async {
        let conn = Rc::new(crate::test_db::open().unwrap());
        let now = startup_tasks::now_unix();
        set_lyrics_timestamps(&conn, 100, now);
        insert_track(&conn, 1, "/music/old.flac", 100, 100);
        insert_track(&conn, 2, "/music/new.flac", 101, 100);
        let (batch, requests) = controlled_batch(&conn);

        batch.start();

        let request = requests.try_recv().unwrap();
        assert_eq!(request.tracks.len(), 2);
        request.events.try_send(WorkerEvent::Cancelled).unwrap();
        glib::timeout_future(Duration::from_millis(1)).await;
    });
}

#[test]
fn lyr_6_an_empty_narrow_pass_still_advances_the_watermark() {
    let conn = Rc::new(crate::test_db::open().unwrap());
    set_lyrics_timestamps(&conn, 1, startup_tasks::now_unix());
    insert_track(&conn, 1, "/music/old.flac", 1, 1);
    let (batch, requests) = controlled_batch(&conn);
    let states = Rc::new(std::cell::RefCell::new(Vec::new()));
    let states_for_callback = states.clone();
    batch.subscribe_progress(
        || true,
        move |progress| states_for_callback.borrow_mut().push(progress.state),
    );

    batch.start_automatically(
        &reprise_core::library::session::SessionState::default(),
        "/music",
    );

    assert!(requests.is_empty());
    assert_eq!(batch.progress.get().state, LyricsBatchState::Complete);
    assert!(!states.borrow().contains(&LyricsBatchState::Running));
    assert!(startup_tasks::lyrics_watermark(&conn).unwrap() > 1);
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
