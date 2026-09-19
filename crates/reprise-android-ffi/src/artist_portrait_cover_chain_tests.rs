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

/// The chain B3 adds: once the portrait run this rides beside reports
/// `Complete`, the cover pass starts on its own worklist and its progress
/// merges into the same `ArtistPortraitProgressUpdate` Kotlin already reads.
/// The one album here already has local art, so this must never reach the
/// network — proven directly through `start_artist_portrait_backfill_with`'s
/// test seam (B3 review finding 2) rather than assumed from a passing
/// `covers_done == covers_total`, which cannot tell a locally-resolved
/// album from one that silently made a live request.
#[test]
fn the_cover_pass_starts_once_the_portrait_run_completes() {
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

    let network_fetch_calls = Arc::new(AtomicUsize::new(0));
    let counted_network_fetch = Arc::clone(&network_fetch_calls);
    library.start_artist_portrait_backfill_with(
        Box::new(NoopProgressListener),
        Arc::new(move |_, _, _| {
            counted_network_fetch.fetch_add(1, Ordering::Relaxed);
            CoverFetchOutcome::NotFound
        }),
    );

    let mut last = library.artist_portrait_backfill_progress();
    for _ in 0..5_000 {
        last = library.artist_portrait_backfill_progress();
        if last.covers_total > 0 && last.covers_done == last.covers_total {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    assert_eq!(last.covers_total, 1, "one album, one representative track");
    assert_eq!(
        last.covers_done, 1,
        "local art settles the album without a request"
    );
    assert_eq!(
        network_fetch_calls.load(Ordering::Relaxed),
        0,
        "a locally-resolved album must never reach the network fetch",
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
    let first_cover_update = captured.iter().position(|update| update.covers_total > 0);
    assert!(
        first_cover_update.is_some(),
        "the chain must report cover progress at all: {captured:?}"
    );
    let first_cover_update = first_cover_update.unwrap();
    assert!(
        captured[..first_cover_update]
            .iter()
            .all(|update| update.state != ArtistPortraitProgressState::Complete),
        "a Complete update arrived before the chained cover pass reported \
         any progress, a bar that finished and then ran backwards: {captured:?}"
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
