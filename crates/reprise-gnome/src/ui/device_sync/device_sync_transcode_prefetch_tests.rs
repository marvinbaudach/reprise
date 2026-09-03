use super::super::*;

pub(super) fn write_fake_output(path: &std::path::Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, vec![7_u8; 100]).map_err(|error| error.to_string())
}

fn audio_copy_pairs(backend: &FakeBackend) -> Vec<(PathBuf, String)> {
    backend
        .state
        .copy_sources
        .borrow()
        .iter()
        .filter(|(_, target)| target.ends_with(".opus"))
        .cloned()
        .collect()
}

#[test]
fn transcode_ahead_starts_the_next_encode_before_the_current_copy_and_keeps_paths_paired() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1, 2, 3, 4]);
        let backend =
            Rc::new(FakeBackend::new(vec![descriptor("a", true)], 0).with_transcode_delay(20));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        runtime.sync_now("a").unwrap();
        settle().await;

        let operations = backend.state.planned_operations.borrow();
        let first_copy = operations
            .iter()
            .position(|(_, operation)| *operation == "copy")
            .unwrap();
        let second_transcode = operations
            .iter()
            .enumerate()
            .filter(|(_, (_, operation))| *operation == "transcode")
            .nth(1)
            .map(|(index, _)| index)
            .unwrap();
        assert!(second_transcode < first_copy);
        drop(operations);

        let starts = backend.state.transcode_starts.borrow();
        let copies = audio_copy_pairs(&backend);
        assert_eq!(copies.len(), 4);
        for (copied_source, target) in copies {
            let (original, _) = starts
                .iter()
                .find(|(_, staged)| *staged == copied_source)
                .expect("every transcoded copy must use one recorded staged path");
            let track_id = original.file_stem().unwrap().to_string_lossy();
            assert!(
                target.contains(&format!("Track {track_id}.opus")),
                "{copied_source:?} was copied to the wrong track target {target}"
            );
        }
    });
}

#[test]
fn cancelling_waits_for_the_encoder_before_discarding_its_staged_output() {
    run(async {
        let temp = tempfile::tempdir().unwrap();
        let staged_path = temp.path().join("prefetched.opus");
        write_fake_output(&staged_path).unwrap();

        let cleanup = cancel_prefetch_for_test(staged_path.clone()).await;

        assert!(cleanup.cancelled);
        assert!(cleanup.pending_drained);
        assert!(cleanup.existed_until_encoder_stopped);
        assert!(
            !staged_path.exists(),
            "cancel_all must discard the staged output after the encoder stops"
        );
    });
}

#[test]
fn an_unprefetched_transcode_runs_inline_and_produces_the_staged_file() {
    run(async {
        let (temp, _conn) = fixture();
        let source = temp.path().join("1.flac");
        let backend = FakeBackend::new(vec![], 0);
        let entry = reprise_core::device_sync::DesiredManagedFile {
            track: reprise_core::device_sync::SyncTrack {
                id: 1,
                source_path: source,
                original_name: "1.flac".into(),
                title: "Track 1".into(),
                artist: "Artist".into(),
                album: "Album".into(),
                album_artist: "Artist".into(),
                track_number: Some(1),
                duration_ms: 1_000,
                bitrate_kbps: Some(1_000),
                size_bytes: 100,
                source_mtime: 0,
            },
            device_path: "Artist/Album/01 Track 1.opus".into(),
            target_bytes: 100,
            profile_fingerprint: "opus-160".into(),
            action: reprise_core::device_sync::TransferAction::TranscodeOpus160,
        };

        let output = transcode_without_prefetch_for_test(
            &backend,
            "a",
            &entry,
            reprise_core::device_sync::TransferAction::TranscodeOpus160,
        )
        .await
        .unwrap();

        assert_eq!(backend.state.transcode_starts.borrow().len(), 1);
        assert!(output.path.exists());
        reprise_core::device_sync::staging::discard(&output.path);
    });
}

#[test]
fn a_superseded_run_discards_a_transcode_completed_before_its_ownership_check() {
    run(async {
        let (temp, conn) = fixture();
        select_road_playlist(&conn, &[1]);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 0));
        let staged_path = temp.path().join("completed-before-supersession.opus");
        backend.return_transcode_from(staged_path.clone());
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        let weak_runtime = Rc::downgrade(&runtime);
        backend.observe_transcode_completion(Rc::new(move |_| {
            if let Some(runtime) = weak_runtime.upgrade() {
                runtime.supersede_current_run_for_test("a");
            }
        }));
        settle().await;

        runtime.sync_now("a").unwrap();
        settle().await;

        assert!(
            !staged_path.exists(),
            "the superseded run's Drop must drain completed transcodes"
        );
    });
}

#[test]
fn a_prefetched_failure_is_reported_when_reached_and_keeps_inline_failure_semantics() {
    run(async {
        let (temp, conn) = fixture();
        select_road_playlist(&conn, &[1, 2, 3]);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 0));
        backend.fail_transcode_for(temp.path().join("2.flac"), "injected transcode failure");
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        runtime.sync_now("a").unwrap();
        settle().await;

        let device = runtime.devices().remove(0);
        assert_eq!(
            device.sync_error.as_ref().unwrap().failed_tracks,
            [2],
            "the same track fails at the same machine event as an inline encode"
        );
        let targets = audio_copy_pairs(&backend)
            .into_iter()
            .map(|(_, target)| target)
            .collect::<Vec<_>>();
        assert_eq!(targets.len(), 2);
        assert!(targets.iter().any(|target| target.contains("Track 1.opus")));
        assert!(targets.iter().any(|target| target.contains("Track 3.opus")));
        assert!(!targets.iter().any(|target| target.contains("Track 2.opus")));

        let operations = backend.state.planned_operations.borrow();
        let first_copy = operations
            .iter()
            .position(|(_, operation)| *operation == "copy")
            .unwrap();
        let failed_prefetch = operations
            .iter()
            .enumerate()
            .filter(|(_, (_, operation))| *operation == "transcode")
            .nth(1)
            .map(|(index, _)| index)
            .unwrap();
        assert!(failed_prefetch < first_copy);
    });
}
