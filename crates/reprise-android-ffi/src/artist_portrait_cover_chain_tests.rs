//! Tests for the chained cover-backfill pass (B3) that starts once the
//! artist-portrait backfill completes.

use super::*;

/// A `SafSource` reading straight off the real filesystem, for tests that
/// need a configured tree — the cover chain (B3) has no reason to run
/// without one.
struct ChainFsSource;

impl crate::source::SafSource for ChainFsSource {
    fn residence_token(&self, _uri: String) -> Result<Option<i64>, crate::source::SafSourceError> {
        Ok(Some(1))
    }

    fn probe(
        &self,
        uri: String,
        _follow_links: bool,
    ) -> Result<Option<crate::source::SourceFacts>, crate::source::SafSourceError> {
        let path = PathBuf::from(&uri);
        Ok(path.exists().then(|| crate::source::SourceFacts {
            display_name: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
            is_file: path.is_file(),
            is_directory: path.is_dir(),
            size_bytes: path.metadata().ok().map(|metadata| metadata.len()),
            modified_unix_ms: None,
            document_id: uri,
        }))
    }

    fn list_children(
        &self,
        uri: String,
    ) -> Result<Vec<crate::source::SourceChild>, crate::source::SafSourceError> {
        let Ok(entries) = std::fs::read_dir(&uri) else {
            return Ok(Vec::new());
        };
        Ok(entries
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                Some(crate::source::SourceChild {
                    uri: path.to_string_lossy().into_owned(),
                    display_name: path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned()),
                    is_file: path.is_file(),
                    is_directory: path.is_dir(),
                    size_bytes: path.metadata().ok().map(|metadata| metadata.len()),
                    modified_unix_ms: None,
                    document_id: path.to_string_lossy().into_owned(),
                })
            })
            .collect())
    }

    fn open_read_fd(&self, uri: String) -> Result<i32, crate::source::SafSourceError> {
        use std::os::fd::IntoRawFd;
        std::fs::File::open(uri)
            .map(IntoRawFd::into_raw_fd)
            .map_err(|error| crate::source::SafSourceError::Io {
                detail: error.to_string(),
            })
    }
}

struct NoopProgressListener;

impl ArtistPortraitProgressListener for NoopProgressListener {
    fn on_progress(&self, _update: ArtistPortraitProgressUpdate) {}
}

struct CapturedProgress(Arc<Mutex<Vec<ArtistPortraitProgressUpdate>>>);

impl ArtistPortraitProgressListener for CapturedProgress {
    fn on_progress(&self, update: ArtistPortraitProgressUpdate) {
        self.0.lock().unwrap().push(update);
    }
}

/// The chain B3 adds: once the portrait run this rides beside reports
/// `Complete`, the cover pass starts on its own worklist and its progress
/// merges into the same `ArtistPortraitProgressUpdate` Kotlin already reads.
/// The one album here already has local art, so this must never reach the
/// network — proven directly through `start_artist_portrait_backfill_with`'s
/// test seam (B3 review finding 2) rather than assumed from a passing
/// `covers_done == covers_total`, which cannot tell a locally-resolved
/// album from one that silently made a live request.
#[test]
fn net_7e_local_art_is_recorded_and_excluded_before_the_cover_pass_counts_work() {
    let _guard = album_cover::reset_album_cover_state_for_tests();
    let directory = tempfile::tempdir().unwrap();
    let music = directory.path().join("music");
    std::fs::create_dir(&music).unwrap();
    let track_path = music.join("track.flac");
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../android/app/src/main/assets/sine.flac");
    std::fs::copy(fixture, &track_path).unwrap();
    reprise_core::library::tag_edit::apply_patch_to_file(
        &track_path,
        &reprise_core::library::tag_edit::TagPatch {
            title: Some("Track One".to_owned()),
            artist: Some("Chain Band".to_owned()),
            album: Some("Chain Album".to_owned()),
            album_artist: Some("Chain Band".to_owned()),
            year: None,
            track_no: Some(Some(1)),
            genre: None,
        },
    )
    .unwrap();
    // Local art: the cover pass must settle this album without a request.
    std::fs::write(music.join("cover.png"), TINY_IMAGE).unwrap();

    let cache_root = directory.path().join("cache");
    let library = MusicLibrary::open_with_portrait_fetch(
        directory.path().to_str().unwrap(),
        cache_root.to_str().unwrap(),
        |_, _| Ok(reprise_core::artist_portrait::PortraitOutcome::NotFound),
    )
    .unwrap();
    open_gate(&library);
    library
        .set_tree_uri(
            music.to_string_lossy().into_owned(),
            Box::new(ChainFsSource),
        )
        .unwrap();
    let writer = library.writer().unwrap();
    reprise_core::library::scanner::scan_folder(&writer, &music).unwrap();
    drop(writer);
    reprise_core::artist_portrait::store_fixture_image(
        &library.portrait_dir(),
        "Chain Band",
        TINY_IMAGE,
        "png",
    )
    .unwrap();

    let updates = Arc::new(Mutex::new(Vec::new()));
    let network_fetch_calls = Arc::new(AtomicUsize::new(0));
    let counted_network_fetch = Arc::clone(&network_fetch_calls);
    library.start_artist_portrait_backfill_with(
        Box::new(CapturedProgress(Arc::clone(&updates))),
        Arc::new(move |_, _, _| {
            counted_network_fetch.fetch_add(1, Ordering::Relaxed);
            CoverFetchOutcome::NotFound
        }),
    );

    let local_marker = reprise_core::cover_download::downloaded_dir_in(&cache_root).join(format!(
        "{}.local",
        reprise_core::cover_download::album_key("Chain Band", "Chain Album")
    ));
    for _ in 0..5_000 {
        if local_marker.exists() && updates.lock().unwrap().len() >= 2 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    assert!(
        local_marker.exists(),
        "local art is remembered during triage"
    );
    assert_eq!(
        network_fetch_calls.load(Ordering::Relaxed),
        0,
        "a locally-resolved album must never reach the network fetch",
    );
    assert!(updates.lock().unwrap().iter().all(|update| {
        update.run_id == 0
            || (update.state == ArtistPortraitProgressState::Complete && update.total == 0)
    }));
    let first_run_id = updates.lock().unwrap().last().unwrap().run_id;
    let marker_time = std::fs::metadata(&local_marker)
        .unwrap()
        .modified()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    library.start_artist_portrait_backfill_with(
        Box::new(CapturedProgress(Arc::clone(&updates))),
        Arc::new(|_, _, _| panic!("local art is not fetched")),
    );
    for _ in 0..5_000 {
        let settled = updates.lock().unwrap().last().is_some_and(|update| {
            update.run_id > first_run_id && update.state == ArtistPortraitProgressState::Complete
        });
        if settled {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(
        std::fs::metadata(&local_marker)
            .unwrap()
            .modified()
            .unwrap(),
        marker_time,
        "a fresh local marker prevents another SAF resolution",
    );
    assert_eq!(library.artist_portrait_backfill_progress().run_id, 0);
}

#[test]
fn net_7e_fresh_portraits_still_chain_one_album_request_and_terminal_snapshot_is_hidden() {
    let _guard = album_cover::reset_album_cover_state_for_tests();
    let directory = tempfile::tempdir().unwrap();
    let music = directory.path().join("music");
    std::fs::create_dir(&music).unwrap();
    let track_path = music.join("track.flac");
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../android/app/src/main/assets/sine.flac");
    std::fs::copy(fixture, &track_path).unwrap();
    reprise_core::library::tag_edit::apply_patch_to_file(
        &track_path,
        &reprise_core::library::tag_edit::TagPatch {
            title: Some("Track".into()),
            artist: Some("Fresh Band".into()),
            album: Some("New Album".into()),
            album_artist: Some("Fresh Band".into()),
            year: None,
            track_no: Some(Some(1)),
            genre: None,
        },
    )
    .unwrap();
    let cache_root = directory.path().join("cache");
    let library = MusicLibrary::open_with_portrait_fetch(
        directory.path().to_str().unwrap(),
        cache_root.to_str().unwrap(),
        |_, _| panic!("a fresh portrait is not fetched"),
    )
    .unwrap();
    open_gate(&library);
    reprise_core::artist_portrait::store_fixture_image(
        &library.portrait_dir(),
        "Fresh Band",
        TINY_IMAGE,
        "png",
    )
    .unwrap();
    library
        .set_tree_uri(
            music.to_string_lossy().into_owned(),
            Box::new(ChainFsSource),
        )
        .unwrap();
    let writer = library.writer().unwrap();
    reprise_core::library::scanner::scan_folder(&writer, &music).unwrap();
    drop(writer);

    let updates = Arc::new(Mutex::new(Vec::new()));
    let fetches = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&fetches);
    library.start_artist_portrait_backfill_with(
        Box::new(CapturedProgress(Arc::clone(&updates))),
        Arc::new(move |_, _, _| {
            counted.fetch_add(1, Ordering::Relaxed);
            CoverFetchOutcome::NotFound
        }),
    );
    for _ in 0..5_000 {
        if fetches.load(Ordering::Relaxed) == 1
            && updates
                .lock()
                .unwrap()
                .last()
                .is_some_and(|update| update.state == ArtistPortraitProgressState::Complete)
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    let updates = updates.lock().unwrap();
    assert!(updates
        .iter()
        .any(|update| update.run_id != 0 && update.covers_total == 1));
    assert_eq!(fetches.load(Ordering::Relaxed), 1);
    let snapshot = library.artist_portrait_backfill_progress();
    assert!(
        snapshot.run_id == 0
            || (snapshot.state == ArtistPortraitProgressState::Complete && snapshot.total == 0)
    );
}

#[test]
fn stopping_portraits_does_not_block_a_later_cover_pass() {
    let _guard = album_cover::reset_album_cover_state_for_tests();
    let directory = tempfile::tempdir().unwrap();
    let music = directory.path().join("music");
    std::fs::create_dir(&music).unwrap();
    let track_path = music.join("track.flac");
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../android/app/src/main/assets/sine.flac");
    std::fs::copy(fixture, &track_path).unwrap();
    reprise_core::library::tag_edit::apply_patch_to_file(
        &track_path,
        &reprise_core::library::tag_edit::TagPatch {
            title: Some("Track One".to_owned()),
            artist: Some("Cancelled Band".to_owned()),
            album: Some("Later Album".to_owned()),
            album_artist: Some("Cancelled Band".to_owned()),
            year: None,
            track_no: Some(Some(1)),
            genre: None,
        },
    )
    .unwrap();

    let entered = Arc::new((Mutex::new(false), Condvar::new()));
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let fetch_entered = Arc::clone(&entered);
    let fetch_release = Arc::clone(&release);
    let library = MusicLibrary::open_with_portrait_fetch(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
        move |_, _| {
            let (entered_lock, entered_wake) = &*fetch_entered;
            *entered_lock.lock().unwrap() = true;
            entered_wake.notify_all();
            let (release_lock, release_wake) = &*fetch_release;
            let mut open = release_lock.lock().unwrap();
            while !*open {
                open = release_wake.wait(open).unwrap();
            }
            Ok(reprise_core::artist_portrait::PortraitOutcome::NotFound)
        },
    )
    .unwrap();
    open_gate(&library);
    library
        .set_tree_uri(
            music.to_string_lossy().into_owned(),
            Box::new(ChainFsSource),
        )
        .unwrap();
    let writer = library.writer().unwrap();
    reprise_core::library::scanner::scan_folder(&writer, &music).unwrap();
    drop(writer);

    let chained_cover_calls = Arc::new(AtomicUsize::new(0));
    let counted_chained_cover_calls = Arc::clone(&chained_cover_calls);
    library.start_artist_portrait_backfill_with(
        Box::new(NoopProgressListener),
        Arc::new(move |_, _, _| {
            counted_chained_cover_calls.fetch_add(1, Ordering::Relaxed);
            CoverFetchOutcome::NotFound
        }),
    );
    let (entered_lock, entered_wake) = &*entered;
    let mut did_enter = entered_lock.lock().unwrap();
    while !*did_enter {
        did_enter = entered_wake.wait(did_enter).unwrap();
    }
    drop(did_enter);

    library.cancel_artist_portrait_backfill();
    let (release_lock, release_wake) = &*release;
    *release_lock.lock().unwrap() = true;
    release_wake.notify_all();
    let database_path = library.database_path.clone();
    drop(library);

    let cover_calls = Arc::new(AtomicUsize::new(0));
    let counted_cover_calls = Arc::clone(&cover_calls);
    let started = album_cover::cover_backfill().start(
        database_path,
        Arc::new(move |_, _, _| {
            counted_cover_calls.fetch_add(1, Ordering::Relaxed);
            CoverFetchOutcome::NotFound
        }),
        Arc::new(|_| {}),
        Arc::new(|| true),
    );

    assert!(
        started,
        "cancelling a portrait run must not leave a latch for an unrelated cover pass"
    );
    for _ in 0..5_000 {
        let progress = album_cover::cover_backfill().progress();
        if progress.total > 0 && progress.done == progress.total {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(cover_calls.load(Ordering::Relaxed), 1);
    assert_eq!(
        chained_cover_calls.load(Ordering::Relaxed),
        0,
        "the stopped portrait run must not start its chained cover pass"
    );
    assert_eq!(
        album_cover::cover_backfill().progress(),
        CoverBackfillProgress { done: 1, total: 1 }
    );
}

#[test]
fn stopping_while_idle_does_not_block_the_next_cover_pass() {
    let _guard = album_cover::reset_album_cover_state_for_tests();
    let directory = tempfile::tempdir().unwrap();
    let library = MusicLibrary::open_with_portrait_fetch(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
        |_, _| Ok(reprise_core::artist_portrait::PortraitOutcome::NotFound),
    )
    .unwrap();
    scan_artists(directory.path(), &library, &["Idle Band"]);

    library.cancel_artist_portrait_backfill();

    let cover_calls = Arc::new(AtomicUsize::new(0));
    let counted_cover_calls = Arc::clone(&cover_calls);
    let started = album_cover::cover_backfill().start(
        library.database_path.clone(),
        Arc::new(move |_, _, _| {
            counted_cover_calls.fetch_add(1, Ordering::Relaxed);
            CoverFetchOutcome::NotFound
        }),
        Arc::new(|_| {}),
        Arc::new(|| true),
    );

    assert!(started, "an idle stop must leave no cancellation behind");
    for _ in 0..5_000 {
        let progress = album_cover::cover_backfill().progress();
        if progress.total > 0 && progress.done == progress.total {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(cover_calls.load(Ordering::Relaxed), 1);
}

#[test]
fn stopping_twice_during_one_portrait_run_does_not_block_the_next_cover_pass() {
    let _guard = album_cover::reset_album_cover_state_for_tests();
    let directory = tempfile::tempdir().unwrap();
    let entered = Arc::new((Mutex::new(false), Condvar::new()));
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let fetch_entered = Arc::clone(&entered);
    let fetch_release = Arc::clone(&release);
    let library = MusicLibrary::open_with_portrait_fetch(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
        move |_, _| {
            let (entered_lock, entered_wake) = &*fetch_entered;
            *entered_lock.lock().unwrap() = true;
            entered_wake.notify_all();
            let (release_lock, release_wake) = &*fetch_release;
            let mut released = release_lock.lock().unwrap();
            while !*released {
                released = release_wake.wait(released).unwrap();
            }
            Ok(reprise_core::artist_portrait::PortraitOutcome::NotFound)
        },
    )
    .unwrap();
    scan_artists(directory.path(), &library, &["Double Stop Band"]);
    open_gate(&library);
    library.start_artist_portrait_backfill(Box::new(NoopProgressListener));

    let (entered_lock, entered_wake) = &*entered;
    let mut did_enter = entered_lock.lock().unwrap();
    while !*did_enter {
        did_enter = entered_wake.wait(did_enter).unwrap();
    }
    drop(did_enter);

    library.cancel_artist_portrait_backfill();
    library.cancel_artist_portrait_backfill();
    let (release_lock, release_wake) = &*release;
    *release_lock.lock().unwrap() = true;
    release_wake.notify_all();
    let database_path = library.database_path.clone();
    drop(library);

    let cover_calls = Arc::new(AtomicUsize::new(0));
    let counted_cover_calls = Arc::clone(&cover_calls);
    let started = album_cover::cover_backfill().start(
        database_path,
        Arc::new(move |_, _, _| {
            counted_cover_calls.fetch_add(1, Ordering::Relaxed);
            CoverFetchOutcome::NotFound
        }),
        Arc::new(|_| {}),
        Arc::new(|| true),
    );

    assert!(
        started,
        "a second stop must not leave a cancellation behind"
    );
    for _ in 0..5_000 {
        let progress = album_cover::cover_backfill().progress();
        if progress.total > 0 && progress.done == progress.total {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(cover_calls.load(Ordering::Relaxed), 1);
}

#[test]
fn stopping_between_portrait_completion_and_cover_start_reports_idle() {
    let _guard = album_cover::reset_album_cover_state_for_tests();
    let directory = tempfile::tempdir().unwrap();
    let music = directory.path().join("music");
    let library = MusicLibrary::open_with_portrait_fetch(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
        |_, _| Ok(reprise_core::artist_portrait::PortraitOutcome::NotFound),
    )
    .unwrap();
    scan_artists(directory.path(), &library, &["Transition Band"]);
    open_gate(&library);
    library
        .set_tree_uri(
            music.to_string_lossy().into_owned(),
            Box::new(ChainFsSource),
        )
        .unwrap();

    let updates = Arc::new(Mutex::new(Vec::new()));
    let entered = Arc::new((Mutex::new(false), Condvar::new()));
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let hook_entered = Arc::clone(&entered);
    let hook_release = Arc::clone(&release);
    install_before_chained_start_hook(move || {
        let (entered_lock, entered_wake) = &*hook_entered;
        *entered_lock.lock().unwrap() = true;
        entered_wake.notify_all();
        let (release_lock, release_wake) = &*hook_release;
        let mut released = release_lock.lock().unwrap();
        while !*released {
            released = release_wake.wait(released).unwrap();
        }
    });
    let cover_calls = Arc::new(AtomicUsize::new(0));
    let counted_cover_calls = Arc::clone(&cover_calls);
    library.start_artist_portrait_backfill_with(
        Box::new(CapturingProgress(Arc::clone(&updates))),
        Arc::new(move |_, _, _| {
            counted_cover_calls.fetch_add(1, Ordering::Relaxed);
            CoverFetchOutcome::NotFound
        }),
    );

    let (entered_lock, entered_wake) = &*entered;
    let mut did_enter = entered_lock.lock().unwrap();
    while !*did_enter {
        did_enter = entered_wake.wait(did_enter).unwrap();
    }
    drop(did_enter);
    assert_eq!(
        library.portrait_backfill.progress().state,
        PortraitBackfillState::Complete,
        "the stop is injected after portrait finish cleared its active flag",
    );
    library.cancel_artist_portrait_backfill();
    let (release_lock, release_wake) = &*release;
    *release_lock.lock().unwrap() = true;
    release_wake.notify_all();

    for _ in 0..5_000 {
        if updates
            .lock()
            .unwrap()
            .last()
            .is_some_and(|update| update.run_id == 0)
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(cover_calls.load(Ordering::Relaxed), 0);
    assert_eq!(
        updates.lock().unwrap().last().copied(),
        Some(ArtistPortraitProgressUpdate {
            run_id: 0,
            state: ArtistPortraitProgressState::Complete,
            done: 0,
            failed: 0,
            total: 0,
            covers_done: 0,
            covers_total: 0,
        }),
        "a cancelled chained start must replace the transitional Running update",
    );
}

/// The live push in `forward` (as opposed to the pure `merged_progress_update`
/// it is built on, exercised separately below) must honour the same
/// invariant: never `Complete` while a chained cover pass has not yet
/// reported any progress of its own (B3 review finding 1). Before the fix
/// this call site pushed the raw portrait `Complete`/`covers=0/0` update
/// unconditionally, ahead of ever starting the cover pass that immediately
/// revokes it.
#[test]
fn the_forward_closure_never_pushes_complete_before_the_chained_cover_pass_reports_progress() {
    let _guard = album_cover::reset_album_cover_state_for_tests();
    let directory = tempfile::tempdir().unwrap();
    let music = directory.path().join("music");
    std::fs::create_dir(&music).unwrap();
    let track_path = music.join("track.flac");
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../android/app/src/main/assets/sine.flac");
    std::fs::copy(fixture, &track_path).unwrap();
    reprise_core::library::tag_edit::apply_patch_to_file(
        &track_path,
        &reprise_core::library::tag_edit::TagPatch {
            title: Some("Track One".to_owned()),
            artist: Some("Push Order Band".to_owned()),
            album: Some("Push Order Album".to_owned()),
            album_artist: Some("Push Order Band".to_owned()),
            year: None,
            track_no: Some(Some(1)),
            genre: None,
        },
    )
    .unwrap();
    std::fs::write(music.join("cover.png"), TINY_IMAGE).unwrap();

    let library = MusicLibrary::open_with_portrait_fetch(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
        |_, _| Ok(reprise_core::artist_portrait::PortraitOutcome::NotFound),
    )
    .unwrap();
    open_gate(&library);
    library
        .set_tree_uri(
            music.to_string_lossy().into_owned(),
            Box::new(ChainFsSource),
        )
        .unwrap();
    let writer = library.writer().unwrap();
    reprise_core::library::scanner::scan_folder(&writer, &music).unwrap();
    drop(writer);

    let updates = Arc::new(Mutex::new(Vec::new()));
    library.start_artist_portrait_backfill(Box::new(CapturingProgress(Arc::clone(&updates))));

    // Polled off the captured log itself, not `artist_portrait_backfill_progress()`:
    // that getter reads the shared state directly, which the worker writes
    // *before* it calls the listener, so polling it and then reading
    // `updates` is its own TOCTOU race under load — this test's assertion
    // only cares what the listener actually saw and in what order.
    for _ in 0..5_000 {
        let settled = updates
            .lock()
            .unwrap()
            .iter()
            .any(|update| update.covers_total > 0 && update.covers_done == update.covers_total);
        if settled {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    let captured = updates.lock().unwrap().clone();
    assert!(
        captured
            .iter()
            .any(|update| update.state == ArtistPortraitProgressState::Complete),
        "the locally-resolved run must terminate: {captured:?}"
    );
    assert!(
        captured.iter().all(|update| update.covers_total == 0),
        "local art is excluded before cover progress is counted: {captured:?}"
    );
}

/// A finished portrait run reports `Complete` — until the cover pass that
/// rides beside it still has albums left, at which point the merged update
/// must not look finished: a `Complete` bar with a `total` that keeps
/// growing reads as a counter running backwards.
#[test]
fn a_portrait_completion_does_not_report_complete_while_covers_are_still_running() {
    let portrait = reprise_core::artist_portrait::PortraitBackfillProgress {
        run_id: 1,
        state: PortraitBackfillState::Complete,
        done: 200,
        failed: 0,
        total: 200,
    };

    let mid_run = merged_progress_update(
        portrait,
        CoverBackfillProgress {
            done: 0,
            total: 500,
        },
    );
    assert_eq!(mid_run.state, ArtistPortraitProgressState::Running);

    let covers_done_too = merged_progress_update(
        portrait,
        CoverBackfillProgress {
            done: 500,
            total: 500,
        },
    );
    assert_eq!(covers_done_too.state, ArtistPortraitProgressState::Complete);

    let no_covers_at_all = merged_progress_update(portrait, CoverBackfillProgress::default());
    assert_eq!(
        no_covers_at_all.state,
        ArtistPortraitProgressState::Complete
    );
}
