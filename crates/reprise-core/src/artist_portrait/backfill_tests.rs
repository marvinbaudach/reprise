use std::fs::{File, FileTimes};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, UNIX_EPOCH};

use super::*;

fn artists_db(names: &[String]) -> Db {
    let db = Db::open_in_memory().unwrap();
    for (index, name) in names.iter().enumerate() {
        db.conn()
            .execute(
                "INSERT INTO tracks (path, title, artist, album_artist, added_at) \
                 VALUES (?1, ?2, ?3, ?3, 0)",
                rusqlite::params![format!("/{index}.flac"), format!("Track {index}"), name],
            )
            .unwrap();
    }
    db
}

fn artist_names(count: usize) -> Vec<String> {
    (0..count)
        .map(|index| format!("Artist {index:03}"))
        .collect()
}

fn no_wait() -> Arc<WaitForRetry> {
    Arc::new(|control, _| control.cancelled())
}

fn updates() -> (
    Arc<Mutex<Vec<PortraitBackfillProgress>>>,
    Arc<PortraitBackfillListener>,
) {
    let updates = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&updates);
    let listener: Arc<PortraitBackfillListener> = Arc::new(move |progress| {
        captured.lock().unwrap().push(progress);
    });
    (updates, listener)
}

fn wait_for_completion(backfill: &PortraitBackfill) -> PortraitBackfillProgress {
    for _ in 0..5_000 {
        let progress = backfill.progress();
        if progress.state == PortraitBackfillState::Complete && progress.run_id != 0 {
            return progress;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("backfill did not complete: {:?}", backfill.progress());
}

fn wait_for_worker_to_finish(backfill: &PortraitBackfill) {
    for _ in 0..5_000 {
        let finished = backfill
            .worker
            .lock()
            .unwrap()
            .as_ref()
            .is_none_or(std::thread::JoinHandle::is_finished);
        if finished {
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("backfill worker did not finish: {:?}", backfill.progress());
}

fn wait_for_run_start(backfill: &PortraitBackfill) -> PortraitBackfillProgress {
    for _ in 0..5_000 {
        let progress = backfill.progress();
        if progress.run_id != 0 {
            return progress;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("backfill did not start: {:?}", backfill.progress());
}

#[test]
fn total_excludes_fresh_portraits_and_markers_but_includes_an_expired_marker() {
    let names = ["Cached", "Fresh miss", "Expired miss", "Needed"].map(str::to_owned);
    let db = artists_db(&names);
    let cache_dir = tempfile::tempdir().unwrap();
    let portrait = cache::store_image(cache_dir.path(), "Cached", b"image", "jpg").unwrap();
    let now = cache::file_epoch_secs(&portrait) + 1;
    cache::write_negative(cache_dir.path(), "Fresh miss");
    cache::write_negative(cache_dir.path(), "Expired miss");
    let expired = cache::negative_marker_path(cache_dir.path(), "Expired miss");
    File::options()
        .write(true)
        .open(expired)
        .unwrap()
        .set_times(
            FileTimes::new()
                .set_modified(UNIX_EPOCH + Duration::from_secs((now - 8 * 24 * 60 * 60) as u64)),
        )
        .unwrap();

    let artists = pending_artists(&db, cache_dir.path(), now).unwrap();

    assert_eq!(artists, vec!["Expired miss", "Needed"]);
}

#[test]
fn pending_artists_prunes_cache_entries_for_artists_absent_from_the_library() {
    let names = ["Still here".to_owned()];
    let db = artists_db(&names);
    let cache_dir = tempfile::tempdir().unwrap();
    let retained = cache::store_image(cache_dir.path(), "Still here", b"image", "jpg").unwrap();
    let removed = cache::store_image(cache_dir.path(), "Removed", b"image", "png").unwrap();
    cache::write_negative(cache_dir.path(), "Renamed");
    let renamed = cache::negative_marker_path(cache_dir.path(), "Renamed");

    let _ = pending_artists(&db, cache_dir.path(), chrono::Utc::now().timestamp()).unwrap();

    assert!(retained.exists());
    assert!(!removed.exists());
    assert!(!renamed.exists());
}

#[test]
fn a_completed_run_leaves_no_work_or_second_request() {
    let cache_dir = tempfile::tempdir().unwrap();
    let names = vec!["Band".to_owned()];
    let db = artists_db(&names);
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let fetch: Arc<PortraitBackfillFetch> = Arc::new(move |name, directory| {
        counted.fetch_add(1, Ordering::Relaxed);
        Ok(PortraitOutcome::Found(
            cache::store_image(directory, name, b"image", "jpg").unwrap(),
        ))
    });
    let (_, listener) = updates();
    let backfill = PortraitBackfill::new();

    assert!(backfill.start_prepared(
        names,
        cache_dir.path().to_owned(),
        Arc::clone(&fetch),
        Arc::clone(&listener),
        no_wait(),
    ));
    wait_for_completion(&backfill);
    let second = pending_artists(&db, cache_dir.path(), chrono::Utc::now().timestamp()).unwrap();
    assert!(second.is_empty());
    assert!(backfill.start_prepared(
        second,
        cache_dir.path().to_owned(),
        fetch,
        listener,
        no_wait(),
    ));
    wait_for_worker_to_finish(&backfill);

    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[test]
fn one_hundred_fast_steps_are_throttled_but_complete_is_always_reported() {
    let cache_dir = tempfile::tempdir().unwrap();
    let (updates, listener) = updates();
    let backfill = PortraitBackfill::new();
    let fetch: Arc<PortraitBackfillFetch> =
        Arc::new(|name, directory| Ok(PortraitOutcome::Found(directory.join(name))));

    backfill.start_prepared(
        artist_names(100),
        cache_dir.path().to_owned(),
        fetch,
        listener,
        no_wait(),
    );
    let complete = wait_for_completion(&backfill);
    let updates = updates.lock().unwrap();

    assert!(updates.len() <= 5, "updates: {updates:?}");
    assert_eq!(updates.last(), Some(&complete));
    assert_eq!(
        (complete.done, complete.failed, complete.total),
        (100, 0, 100)
    );
}

#[test]
fn an_empty_worklist_publishes_no_progress() {
    let cache_dir = tempfile::tempdir().unwrap();
    let (updates, listener) = updates();
    let backfill = PortraitBackfill::new();
    let fetch: Arc<PortraitBackfillFetch> = Arc::new(|_, _| panic!("empty work must not fetch"));

    assert!(backfill.start_prepared(
        Vec::new(),
        cache_dir.path().to_owned(),
        fetch,
        listener,
        no_wait(),
    ));
    wait_for_worker_to_finish(&backfill);

    assert_eq!(backfill.progress(), PortraitBackfillProgress::idle());
    assert!(updates.lock().unwrap().is_empty());
}

#[test]
fn a_non_empty_worklist_still_publishes_preparing_first() {
    let cache_dir = tempfile::tempdir().unwrap();
    let (updates, listener) = updates();
    let backfill = PortraitBackfill::new();
    let fetch: Arc<PortraitBackfillFetch> =
        Arc::new(|name, directory| Ok(PortraitOutcome::Found(directory.join(name))));

    assert!(backfill.start_prepared(
        vec!["Band".to_owned()],
        cache_dir.path().to_owned(),
        fetch,
        listener,
        no_wait(),
    ));
    wait_for_completion(&backfill);

    assert_eq!(
        updates.lock().unwrap().first().map(|update| update.state),
        Some(PortraitBackfillState::Preparing)
    );
}

#[test]
fn reported_done_never_moves_backwards() {
    let cache_dir = tempfile::tempdir().unwrap();
    let (updates, listener) = updates();
    let backfill = PortraitBackfill::new();
    let fetch: Arc<PortraitBackfillFetch> = Arc::new(|name, directory| {
        std::thread::sleep(REPORT_INTERVAL);
        Ok(PortraitOutcome::Found(directory.join(name)))
    });
    backfill.start_prepared(
        artist_names(4),
        cache_dir.path().to_owned(),
        fetch,
        listener,
        no_wait(),
    );
    wait_for_completion(&backfill);
    let updates = updates.lock().unwrap();

    assert!(updates.windows(2).all(|pair| pair[0].done <= pair[1].done));
}

#[test]
fn three_transport_errors_pause_and_the_first_ok_resumes() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let fetch: Arc<PortraitBackfillFetch> = Arc::new(move |name, directory| {
        if counted.fetch_add(1, Ordering::Relaxed) < 3 {
            Err(PortraitError::Fetch(crate::musicbrainz::FetchError::Transport))
        } else {
            Ok(PortraitOutcome::Found(directory.join(name)))
        }
    });
    let cache_dir = tempfile::tempdir().unwrap();
    let (updates, listener) = updates();
    let backfill = PortraitBackfill::new();
    backfill.start_prepared(
        vec!["Band".to_owned(), "Next".to_owned()],
        cache_dir.path().to_owned(),
        fetch,
        listener,
        no_wait(),
    );
    wait_for_completion(&backfill);
    let states: Vec<_> = updates
        .lock()
        .unwrap()
        .iter()
        .map(|update| update.state)
        .collect();

    let paused = states
        .iter()
        .position(|state| *state == PortraitBackfillState::Paused)
        .unwrap();
    assert!(states[paused + 1..].contains(&PortraitBackfillState::Running));
}

#[test]
fn transport_errors_are_retried_without_counts_or_negative_marker() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let fetch: Arc<PortraitBackfillFetch> = Arc::new(move |_, _| {
        counted.fetch_add(1, Ordering::Relaxed);
        Err(PortraitError::Fetch(crate::musicbrainz::FetchError::Transport))
    });
    let cache_dir = tempfile::tempdir().unwrap();
    let backfill = Arc::new(PortraitBackfill::new());
    let cancel = Arc::clone(&backfill);
    let wait: Arc<WaitForRetry> = Arc::new(move |_, _| {
        cancel.cancel();
        true
    });
    let (_, listener) = updates();

    backfill.start_prepared(
        vec!["Offline band".to_owned()],
        cache_dir.path().to_owned(),
        fetch,
        listener,
        wait,
    );
    wait_for_worker_to_finish(&backfill);

    assert!(calls.load(Ordering::Relaxed) >= 3);
    assert!(!cache::negative_marker_path(cache_dir.path(), "Offline band").exists());
    assert_eq!(
        (backfill.progress().done, backfill.progress().failed),
        (0, 0)
    );
}

#[test]
fn cancelling_from_another_thread_after_fetch_allows_a_fresh_run() {
    let entered = Arc::new((Mutex::new(false), Condvar::new()));
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let fetch_entered = Arc::clone(&entered);
    let fetch_release = Arc::clone(&release);
    let fetch: Arc<PortraitBackfillFetch> = Arc::new(move |name, directory| {
        let (entered_lock, entered_wake) = &*fetch_entered;
        *entered_lock.lock().unwrap() = true;
        entered_wake.notify_all();
        let (release_lock, release_wake) = &*fetch_release;
        let mut open = release_lock.lock().unwrap();
        while !*open {
            open = release_wake.wait(open).unwrap();
        }
        Ok(PortraitOutcome::Found(directory.join(name)))
    });
    let cache_dir = tempfile::tempdir().unwrap();
    let (_, listener) = updates();
    let backfill = PortraitBackfill::new();
    assert!(backfill.start_prepared(
        vec!["First".to_owned()],
        cache_dir.path().to_owned(),
        fetch,
        listener,
        no_wait(),
    ));
    let (entered_lock, entered_wake) = &*entered;
    let mut did_enter = entered_lock.lock().unwrap();
    while !*did_enter {
        did_enter = entered_wake.wait(did_enter).unwrap();
    }

    backfill.cancel();
    let (release_lock, release_wake) = &*release;
    *release_lock.lock().unwrap() = true;
    release_wake.notify_all();
    wait_for_worker_to_finish(&backfill);

    let (_, listener) = updates();
    assert!(backfill.start_prepared(
        Vec::new(),
        cache_dir.path().to_owned(),
        Arc::new(|_, _| panic!("empty restart must not fetch")),
        listener,
        no_wait(),
    ));
}

#[test]
fn cancellation_interrupts_the_production_retry_wait() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&attempts);
    let fetch: Arc<PortraitBackfillFetch> = Arc::new(move |_, _| {
        counted.fetch_add(1, Ordering::Relaxed);
        Err(PortraitError::Fetch(crate::musicbrainz::FetchError::Transport))
    });
    let cache_dir = tempfile::tempdir().unwrap();
    let (_, listener) = updates();
    let backfill = PortraitBackfill::new();
    assert!(backfill.start_prepared(
        vec!["Offline".to_owned()],
        cache_dir.path().to_owned(),
        fetch,
        listener,
        Arc::new(Control::wait),
    ));
    for _ in 0..5_000 {
        if backfill.progress().state == PortraitBackfillState::Paused {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(backfill.progress().state, PortraitBackfillState::Paused);

    let started = std::time::Instant::now();
    backfill.cancel();
    wait_for_worker_to_finish(&backfill);

    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(backfill.progress(), PortraitBackfillProgress::idle());
    assert_eq!(attempts.load(Ordering::Relaxed), 3);
}

#[test]
fn a_panicking_worker_is_logged_and_does_not_block_the_next_run() {
    let cache_dir = tempfile::tempdir().unwrap();
    let (_, listener) = updates();
    let backfill = PortraitBackfill::new();
    assert!(backfill.start_prepared(
        vec!["Panic".to_owned()],
        cache_dir.path().to_owned(),
        Arc::new(|_, _| panic!("intentional fetch panic")),
        listener,
        no_wait(),
    ));
    wait_for_worker_to_finish(&backfill);
    let logs = crate::log_capture::CapturedLogs::default();
    let (_, listener) = updates();

    let restarted = logs.capture(|| {
        backfill.start_prepared(
            Vec::new(),
            cache_dir.path().to_owned(),
            Arc::new(|_, _| panic!("empty restart must not fetch")),
            listener,
            no_wait(),
        )
    });

    assert!(restarted);
    assert!(
        logs.joined()
            .contains("artist portrait backfill worker panicked"),
        "missing panic diagnostic: {}",
        logs.joined()
    );
}

#[test]
fn run_ids_grow_and_a_second_start_does_not_create_a_worker() {
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let waiting = Arc::clone(&gate);
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let fetch: Arc<PortraitBackfillFetch> = Arc::new(move |name, directory| {
        counted.fetch_add(1, Ordering::Relaxed);
        let (lock, wake) = &*waiting;
        let mut open = lock.lock().unwrap();
        while !*open {
            open = wake.wait(open).unwrap();
        }
        Ok(PortraitOutcome::Found(directory.join(name)))
    });
    let cache_dir = tempfile::tempdir().unwrap();
    let (_, first_listener) = updates();
    let (_, second_listener) = updates();
    let backfill = PortraitBackfill::new();
    assert!(backfill.start_prepared(
        vec!["First".to_owned()],
        cache_dir.path().to_owned(),
        Arc::clone(&fetch),
        first_listener,
        no_wait(),
    ));
    let first_id = wait_for_run_start(&backfill).run_id;
    assert!(!backfill.start_prepared(
        vec!["Duplicate".to_owned()],
        cache_dir.path().to_owned(),
        Arc::clone(&fetch),
        second_listener,
        no_wait(),
    ));
    let (lock, wake) = &*gate;
    *lock.lock().unwrap() = true;
    wake.notify_all();
    wait_for_completion(&backfill);
    let (_, listener) = updates();
    assert!(backfill.start_prepared(
        vec!["Second".to_owned()],
        cache_dir.path().to_owned(),
        fetch,
        listener,
        no_wait(),
    ));
    let second_id = wait_for_completion(&backfill).run_id;

    assert!(second_id > first_id);
    assert_eq!(calls.load(Ordering::Relaxed), 2);
}

#[test]
fn the_fetch_closure_delivers_transport_errors_to_the_state_machine() {
    let seen_wait = Arc::new(AtomicUsize::new(0));
    let wait_seen = Arc::clone(&seen_wait);
    let wait: Arc<WaitForRetry> = Arc::new(move |control, _| {
        wait_seen.fetch_add(1, Ordering::Relaxed);
        control.cancelled()
    });
    let attempts = Arc::new(AtomicUsize::new(0));
    let tried = Arc::clone(&attempts);
    let fetch: Arc<PortraitBackfillFetch> = Arc::new(move |name, directory| {
        if tried.fetch_add(1, Ordering::Relaxed) < 3 {
            Err(PortraitError::Fetch(crate::musicbrainz::FetchError::Transport))
        } else {
            Ok(PortraitOutcome::Found(directory.join(name)))
        }
    });
    let cache_dir = tempfile::tempdir().unwrap();
    let (_, listener) = updates();
    let backfill = PortraitBackfill::new();
    backfill.start_prepared(
        vec!["Band".to_owned()],
        cache_dir.path().to_owned(),
        fetch,
        listener,
        wait,
    );
    wait_for_completion(&backfill);

    assert_eq!(seen_wait.load(Ordering::Relaxed), 1);
    assert_eq!(attempts.load(Ordering::Relaxed), 4);
}

#[test]
fn one_broken_artist_does_not_block_the_others() {
    let cache_dir = tempfile::tempdir().unwrap();
    let fetch: Arc<PortraitBackfillFetch> = Arc::new(|name, directory| {
        if name == "Bad" {
            Err(PortraitError::InvalidResponse)
        } else {
            Ok(PortraitOutcome::Found(directory.join(name)))
        }
    });
    let (_, listener) = updates();
    let backfill = PortraitBackfill::new();
    backfill.start_prepared(
        vec!["Bad".to_owned(), "A".to_owned(), "B".to_owned()],
        cache_dir.path().to_owned(),
        fetch,
        listener,
        no_wait(),
    );
    let final_progress = wait_for_completion(&backfill);

    assert_eq!(final_progress.state, PortraitBackfillState::Complete);
    assert_eq!(final_progress.done, 2);
    assert_eq!(final_progress.failed, 1);
}

#[test]
fn a_run_of_only_broken_artists_still_finishes() {
    let cache_dir = tempfile::tempdir().unwrap();
    let fetch: Arc<PortraitBackfillFetch> = Arc::new(|_, _| {
        Err(PortraitError::InvalidResponse)
    });
    let (_, listener) = updates();
    let backfill = PortraitBackfill::new();
    backfill.start_prepared(
        vec!["Bad1".to_owned(), "Bad2".to_owned()],
        cache_dir.path().to_owned(),
        fetch,
        listener,
        no_wait(),
    );
    let final_progress = wait_for_completion(&backfill);

    assert_eq!(final_progress.state, PortraitBackfillState::Complete);
    assert_eq!(final_progress.done, 0);
    assert_eq!(final_progress.failed, 2);
}

#[test]
fn a_broken_artist_never_pauses_the_run() {
    let cache_dir = tempfile::tempdir().unwrap();
    let (updates, listener) = updates();
    let fetch: Arc<PortraitBackfillFetch> = Arc::new(|name, directory| {
        if name == "Bad" {
            Err(PortraitError::InvalidResponse)
        } else {
            Ok(PortraitOutcome::Found(directory.join(name)))
        }
    });
    let backfill = PortraitBackfill::new();
    backfill.start_prepared(
        vec!["Bad".to_owned(), "A".to_owned(), "B".to_owned()],
        cache_dir.path().to_owned(),
        fetch,
        listener,
        no_wait(),
    );
    wait_for_completion(&backfill);
    let states: Vec<_> = updates
        .lock()
        .unwrap()
        .iter()
        .map(|update| update.state)
        .collect();

    assert!(!states.contains(&PortraitBackfillState::Paused));
}

#[test]
fn a_dropped_artist_gets_no_negative_marker() {
    let cache_dir = tempfile::tempdir().unwrap();
    let cache_path = cache_dir.path().to_owned();
    let call_count = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&call_count);
    let fetch: Arc<PortraitBackfillFetch> = Arc::new(move |name, _| {
        if name == "Bad" {
            counted.fetch_add(1, Ordering::Relaxed);
            Err(PortraitError::InvalidResponse)
        } else {
            Ok(PortraitOutcome::Found(cache_path.join(name)))
        }
    });
    let (_, listener) = updates();
    let backfill = PortraitBackfill::new();
    backfill.start_prepared(
        vec!["Bad".to_owned(), "A".to_owned()],
        cache_dir.path().to_owned(),
        fetch,
        listener,
        no_wait(),
    );
    wait_for_completion(&backfill);

    let negative_marker = cache::negative_marker_path(cache_dir.path(), "Bad");
    assert!(!negative_marker.exists());
    // Verify it was actually attempted (to distinguish from never-tried artists)
    assert!(call_count.load(Ordering::Relaxed) > 0);
}

#[test]
fn a_total_outage_spends_no_attempt_budget() {
    let cache_dir = tempfile::tempdir().unwrap();
    let fetch: Arc<PortraitBackfillFetch> = Arc::new(|name, directory| {
        if name == "A" || name == "B" {
            // Network-shaped error: Transport
            Err(PortraitError::Fetch(crate::musicbrainz::FetchError::Transport))
        } else {
            Ok(PortraitOutcome::Found(directory.join(name)))
        }
    });
    let (_, listener) = updates();
    let backfill = Arc::new(PortraitBackfill::new());
    let cancel = Arc::clone(&backfill);
    // Count how many times we attempt; if network errors consume budget, we'd exceed this
    let attempt_count = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&attempt_count);
    let wait: Arc<WaitForRetry> = Arc::new(move |_, _| {
        // On the 4th+ attempt, cancel to prevent infinite loop
        // This allows us to verify that network errors don't exhaust artist budgets
        if counted.fetch_add(1, Ordering::Relaxed) >= 3 {
            cancel.cancel();
            true
        } else {
            false
        }
    });
    backfill.start_prepared(
        vec!["A".to_owned(), "B".to_owned(), "C".to_owned()],
        cache_dir.path().to_owned(),
        fetch,
        listener,
        wait,
    );
    wait_for_worker_to_finish(&backfill);
    let final_progress = backfill.progress();

    // A and B experience network-shaped errors and never exhaust budgets.
    // After enough retries, the run is cancelled.
    // If network errors incorrectly consumed budgets, A or B would be dropped.
    // If fixed correctly, they're still "Running" and not dropped.
    assert_eq!(final_progress.failed, 0, "network errors should not result in failed artists");
    assert_eq!(final_progress.done, 0, "network errors should not complete the artists");
}
