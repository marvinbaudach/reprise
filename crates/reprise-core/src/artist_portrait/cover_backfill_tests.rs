use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::*;
use crate::cover_download::CoverFetchOutcome;

/// One row per `(album_artist, album, path)` — enough for `pending_albums`'s
/// `query_albums` read: present, non-blank album.
fn insert_albums(db: &Db, rows: &[(&str, &str, &str)]) {
    for (index, (album_artist, album, path)) in rows.iter().enumerate() {
        db.conn()
            .execute(
                "INSERT INTO tracks (path, title, artist, album, album_artist, added_at) \
                 VALUES (?1, ?2, ?3, ?4, ?3, 0)",
                rusqlite::params![path, format!("Track {index}"), album_artist, album],
            )
            .unwrap();
    }
}

fn always() -> Arc<dyn Fn() -> bool + Send + Sync> {
    Arc::new(|| true)
}

fn updates() -> (
    Arc<Mutex<Vec<CoverBackfillProgress>>>,
    Arc<CoverBackfillListener>,
) {
    let updates = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&updates);
    let listener: Arc<CoverBackfillListener> = Arc::new(move |progress| {
        captured.lock().unwrap().push(progress);
    });
    (updates, listener)
}

fn wait_for_worker_to_finish(backfill: &CoverBackfill) {
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
    panic!(
        "cover backfill worker did not finish: {:?}",
        backfill.progress()
    );
}

#[test]
fn the_cover_pass_starts_after_the_portraits() {
    // Stands in for the FFI chain: once the portrait run this rides beside
    // reports `Complete`, the cover pass is `CoverBackfill::start` — its own
    // worklist read from its own `Db`, exercised here through the real
    // entry point rather than the test-only `start_prepared`.
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("reprise.db");
    {
        let db = Db::open_migrated(Some(&database_path)).unwrap();
        insert_albums(
            &db,
            &[
                ("Band A", "Album A", "/a.flac"),
                ("Band B", "Album B", "/b.flac"),
            ],
        );
    }
    let backfill = CoverBackfill::new();
    let (updates, listener) = updates();
    let fetch: Arc<CoverBackfillFetch> = Arc::new(|_, _, _| CoverFetchOutcome::NotFound);

    let started = backfill.start(database_path, fetch, listener, always());
    wait_for_worker_to_finish(&backfill);

    assert!(started);
    assert_eq!(
        backfill.progress(),
        CoverBackfillProgress { done: 2, total: 2 }
    );
    let last = *updates.lock().unwrap().last().unwrap();
    assert_eq!(last, CoverBackfillProgress { done: 2, total: 2 });
}

#[test]
fn albums_with_local_art_are_skipped() {
    // The fetch closure alone decides whether an album needed a request —
    // this module has no filesystem access to tell. Whatever it reports,
    // the pass advances past the album without pausing or retrying.
    let backfill = CoverBackfill::new();
    let (_, listener) = updates();
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let fetch: Arc<CoverBackfillFetch> = Arc::new(move |_, _, _| {
        counted.fetch_add(1, Ordering::Relaxed);
        // The first album already has local art, reported the same way a
        // fresh download would be; the second is a definitive miss.
        if counted.load(Ordering::Relaxed) == 1 {
            CoverFetchOutcome::Downloaded(std::path::PathBuf::from("/cover.png"))
        } else {
            CoverFetchOutcome::NotFound
        }
    });

    backfill.start_prepared(
        vec![
            (
                "Local Band".into(),
                "Local Album".into(),
                "/local.flac".into(),
            ),
            ("Miss Band".into(), "Miss Album".into(), "/miss.flac".into()),
        ],
        fetch,
        listener,
        always(),
    );
    wait_for_worker_to_finish(&backfill);

    assert_eq!(calls.load(Ordering::Relaxed), 2);
    assert_eq!(
        backfill.progress(),
        CoverBackfillProgress { done: 2, total: 2 }
    );
}

#[test]
fn cancel_stops_the_cover_pass() {
    let backfill = Arc::new(CoverBackfill::new());
    let (updates, listener) = updates();
    let cancelling = Arc::clone(&backfill);
    let fetch: Arc<CoverBackfillFetch> = Arc::new(move |_, _, _| {
        cancelling.cancel();
        CoverFetchOutcome::NotFound
    });

    backfill.start_prepared(
        vec![
            ("Band A".into(), "Album A".into(), "/a.flac".into()),
            ("Band B".into(), "Album B".into(), "/b.flac".into()),
            ("Band C".into(), "Album C".into(), "/c.flac".into()),
        ],
        fetch,
        listener,
        always(),
    );
    wait_for_worker_to_finish(&backfill);

    assert_eq!(backfill.progress(), CoverBackfillProgress::default());
    let last = *updates.lock().unwrap().last().unwrap();
    assert_eq!(
        last,
        CoverBackfillProgress::default(),
        "a cancelled pass must report back to idle, the way the portrait pass does",
    );
}

#[test]
fn a_cancel_before_start_is_honoured_once_start_finally_runs() {
    // The window the FFI closure lives in for a moment on every real
    // completion (B3 review findings 6/7): the run does not exist yet, so
    // `active` is `false`, but a cancel that arrives right then must not
    // be silently discarded by the `start()` that follows it.
    let backfill = CoverBackfill::new();
    let (_, listener) = updates();
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let fetch: Arc<CoverBackfillFetch> = Arc::new(move |_, _, _| {
        counted.fetch_add(1, Ordering::Relaxed);
        CoverFetchOutcome::NotFound
    });

    backfill.cancel();
    let started = backfill.start_prepared(
        vec![("Band A".into(), "Album A".into(), "/a.flac".into())],
        fetch,
        listener,
        always(),
    );

    assert!(!started, "a pending cancel must refuse the next launch");
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    assert_eq!(backfill.progress(), CoverBackfillProgress::default());

    // The refusal is one-shot: an unrelated later start is not blocked by
    // an already-consumed cancel.
    let (_, listener) = updates();
    let later_calls = Arc::new(AtomicUsize::new(0));
    let counted_later = Arc::clone(&later_calls);
    let fetch: Arc<CoverBackfillFetch> = Arc::new(move |_, _, _| {
        counted_later.fetch_add(1, Ordering::Relaxed);
        CoverFetchOutcome::NotFound
    });
    let started_later = backfill.start_prepared(
        vec![("Band A".into(), "Album A".into(), "/a.flac".into())],
        fetch,
        listener,
        always(),
    );
    wait_for_worker_to_finish(&backfill);

    assert!(started_later, "a later, unrelated start must not stay blocked");
    assert_eq!(later_calls.load(Ordering::Relaxed), 1);
}

#[test]
fn consent_withdrawn_mid_run_stops_the_pass() {
    let backfill = CoverBackfill::new();
    let (_, listener) = updates();
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let fetch: Arc<CoverBackfillFetch> = Arc::new(move |_, _, _| {
        counted.fetch_add(1, Ordering::Relaxed);
        CoverFetchOutcome::NotFound
    });
    let allowed = Arc::new(AtomicUsize::new(1));
    let gate = Arc::clone(&allowed);
    let consent_allowed: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || {
        let was_allowed = gate.load(Ordering::Relaxed) == 1;
        gate.store(0, Ordering::Relaxed);
        was_allowed
    });

    backfill.start_prepared(
        vec![
            ("Band A".into(), "Album A".into(), "/a.flac".into()),
            ("Band B".into(), "Album B".into(), "/b.flac".into()),
            ("Band C".into(), "Album C".into(), "/c.flac".into()),
        ],
        fetch,
        listener,
        consent_allowed,
    );
    wait_for_worker_to_finish(&backfill);

    assert_eq!(
        calls.load(Ordering::Relaxed),
        1,
        "consent is rechecked before every album, so only the first ran",
    );
    assert_eq!(backfill.progress(), CoverBackfillProgress::default());
}
