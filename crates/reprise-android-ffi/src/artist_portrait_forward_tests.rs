//! The order of the updates the forward closure pushes while a cover pass rides behind a
//! portrait run that had work (NET-7e).

use super::cover_chain_tests::ChainFsSource;
use super::*;

/// A portrait run with work (the artist is a miss) followed by an album that needs a request
/// (no local art). The portrait `Complete` must reach Kotlin as `Running` until the cover
/// pass reports its own progress: a `Complete` pushed ahead of the cover count is revoked by
/// the next update and reads as a finished bar whose total then grows.
#[test]
fn net_7e_a_portrait_run_with_work_hands_over_to_the_cover_pass_without_a_complete() {
    let _guard = album_cover::reset_album_cover_state_for_tests();
    let directory = tempfile::tempdir().unwrap();
    let music = directory.path().join("music");
    std::fs::create_dir(&music).unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../android/app/src/main/assets/sine.flac");
    let track_path = music.join("track.flac");
    std::fs::copy(fixture, &track_path).unwrap();
    reprise_core::library::tag_edit::apply_patch_to_file(
        &track_path,
        &reprise_core::library::tag_edit::TagPatch {
            title: Some("Track".to_owned()),
            artist: Some("Handover Band".to_owned()),
            album: Some("Handover Album".to_owned()),
            album_artist: Some("Handover Band".to_owned()),
            year: None,
            track_no: Some(Some(1)),
            genre: None,
        },
    )
    .unwrap();
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
    library.start_artist_portrait_backfill_with(
        Box::new(CapturingProgress(Arc::clone(&updates))),
        Arc::new(|_, _, _| CoverFetchOutcome::NotFound),
    );
    for _ in 0..5_000 {
        let settled = updates.lock().unwrap().iter().any(|update| {
            update.covers_total == 1
                && update.covers_done == 1
                && update.state == ArtistPortraitProgressState::Complete
        });
        if settled {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    let captured = updates.lock().unwrap().clone();
    let first_cover_count = captured
        .iter()
        .position(|update| update.covers_total > 0)
        .unwrap_or_else(|| panic!("the cover pass never counted its album: {captured:?}"));
    assert!(
        captured[..first_cover_count]
            .iter()
            .all(|update| update.state != ArtistPortraitProgressState::Complete),
        "a Complete came before the cover pass counted any work: {captured:?}"
    );
    let portrait_completion = captured
        .iter()
        .find(|update| update.total > 0 && update.done + update.failed == update.total)
        .unwrap_or_else(|| panic!("the portrait run never completed: {captured:?}"));
    assert_eq!(
        portrait_completion.state,
        ArtistPortraitProgressState::Running,
        "the first update after the portraits finished must be Running: {captured:?}"
    );
    assert!(
        captured.last().is_some_and(|last| {
            last.state == ArtistPortraitProgressState::Complete
                && last.covers_total == 1
                && last.covers_done == 1
        }),
        "the handover must end in Complete: {captured:?}"
    );
}
