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
fn cancelling_with_prefetches_outstanding_discards_every_staged_output() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1, 2, 3, 4]);
        let backend =
            Rc::new(FakeBackend::new(vec![descriptor("a", true)], 0).with_transcode_delay(200));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        runtime.sync_now("a").unwrap();
        for _ in 0..100 {
            if backend.state.transcode_starts.borrow().len() >= 3 {
                break;
            }
            gtk4::glib::timeout_future(Duration::from_millis(1)).await;
        }
        let staged = backend
            .state
            .transcode_starts
            .borrow()
            .iter()
            .map(|(_, path)| path.clone())
            .collect::<Vec<_>>();
        assert_eq!(staged.len(), 3, "exactly three encodes must be in flight");

        runtime.cancel_current("a");
        settle().await;

        assert!(staged.iter().all(|path| !path.exists()));
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
