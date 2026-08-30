//! Run-log lifecycle tests — split out of `device_sync_planned_tests.rs` to
//! keep that file under the project's 800-line limit, the same way
//! `device_sync_inflight_tests.rs` was.
//!
//! What they cover: the run row exists *while* a transfer runs, every rejected
//! start closes the row it was handed, and the call site that receives an
//! already-open log (the preparation continuation) closes it when the device
//! has gone away.

use super::*;

fn unremembered_start(started_at: i64) -> reprise_core::device_sync::sync_log::RunStart {
    reprise_core::device_sync::sync_log::RunStart {
        device_serial: "mtp://[usb:001,013]/".into(),
        device_name: "Unknown Android phone".into(),
        transfer_profile: "opus_160".into(),
        started_at,
        planned: 1,
    }
}

#[test]
fn runtime_startup_closes_a_run_orphaned_by_the_previous_session() {
    let (_temp, conn) = fixture();
    let orphan =
        reprise_core::device_sync::sync_log::start_run(&conn, &unremembered_start(4_000)).unwrap();
    let backend = Rc::new(FakeBackend::new(Vec::new(), 1));

    let _runtime = DeviceSyncRuntime::with_backend(&conn, backend);

    let runs = reprise_core::device_sync::sync_log::recent_runs(&conn, 10).unwrap();
    let closed = runs.iter().find(|run| run.id == orphan).unwrap();
    assert_eq!(
        closed.outcome,
        reprise_core::device_sync::sync_log::RunOutcome::Interrupted
    );
    assert!(closed.finished_at.is_some());
}

#[test]
fn an_unrememberable_device_run_records_start_outcome_and_deviation() {
    run(async {
        let (_temp, conn) = fixture();
        let backend = Rc::new(FakeBackend::new(
            vec![descriptor("mtp://[usb:001,013]/", false)],
            1,
        ));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        let log = RunLog::open(&runtime, &unremembered_start(4_000));
        let opened = reprise_core::device_sync::sync_log::recent_runs(&conn, 1)
            .unwrap()
            .remove(0);
        assert_eq!(opened.started_at, 4_000);
        assert_eq!(
            opened.outcome,
            reprise_core::device_sync::sync_log::RunOutcome::Running
        );

        reprise_core::device_sync::sync_log::note_deviation(
            &conn,
            opened.id,
            &reprise_core::device_sync::sync_log::Deviation {
                kind: reprise_core::device_sync::sync_log::DeviationKind::Failed,
                track_id: Some(7),
                device_path: "Music/Reprise/Track.opus".into(),
                detail: "copy failed: device disconnected".into(),
            },
        )
        .unwrap();
        log.close(
            &runtime,
            &reprise_core::device_sync::SyncOutcome::Failed {
                terminal_error: Some("device disconnected".into()),
                failed_tracks: vec![7],
                verified_sources: Vec::new(),
            },
            4_100,
        );

        let recorded = reprise_core::device_sync::sync_log::recent_runs(&conn, 1)
            .unwrap()
            .remove(0);
        assert_eq!(recorded.device_serial, "mtp://[usb:001,013]/");
        assert_eq!(recorded.finished_at, Some(4_100));
        assert_eq!(
            recorded.outcome,
            reprise_core::device_sync::sync_log::RunOutcome::Failed
        );
        let deviations =
            reprise_core::device_sync::sync_log::deviations(&conn, recorded.id).unwrap();
        assert_eq!(deviations.len(), 1);
        assert_eq!(deviations[0].track_id, Some(7));
        assert_eq!(deviations[0].detail, "copy failed: device disconnected");
    });
}

#[test]
fn a_live_transfer_writes_a_running_log_row_with_its_final_planned_count() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1]);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let (started, releases) = backend.gate_copies(&["a"]);
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        runtime.sync_now("a").unwrap();
        started.recv().await.unwrap();

        let recorded = reprise_core::device_sync::sync_log::recent_runs(&conn, 1)
            .unwrap()
            .remove(0);
        assert_eq!(
            recorded.outcome,
            reprise_core::device_sync::sync_log::RunOutcome::Running
        );
        assert_eq!(recorded.planned, 1);

        releases["a"].send(()).await.unwrap();
        settle().await;
    });
}

#[test]
fn a_failed_track_is_omitted_when_the_runtime_publishes_its_playlist() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1, 2]);
        crate::test_db::connection(&conn)
            .execute_batch(
                "CREATE TRIGGER reject_first_device_file
                 BEFORE INSERT ON device_files
                 WHEN NEW.track_id = 1
                 BEGIN
                   SELECT RAISE(FAIL, 'injected inventory failure');
                 END;",
            )
            .unwrap();
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        runtime.sync_now("a").unwrap();
        settle().await;

        let playlists = backend.state.playlists.borrow();
        assert_eq!(playlists.len(), 1);
        let contents = String::from_utf8(playlists[0].2.clone()).unwrap();
        assert!(
            !contents.contains("Track 1.opus"),
            "the failed track must not remain in the published m3u: {contents}"
        );
        assert!(
            contents.contains("Track 2.opus"),
            "the other playlist entries must survive: {contents}"
        );
        assert_eq!(
            reprise_core::device_sync::sync_log::recent_runs(&conn, 1)
                .unwrap()
                .remove(0)
                .outcome,
            reprise_core::device_sync::sync_log::RunOutcome::Failed,
            "publishing a safe subset does not turn the run into a success"
        );
        assert!(
            runtime.devices()[0].sync_error.is_some(),
            "the failed run remains reported as failed after verification"
        );
        assert!(
            reprise_core::device_sync::settings::load_device_playlists(&conn, "a")
                .unwrap()
                .into_iter()
                .find(|playlist| playlist.source == SelectionSource::Playlist(10))
                .and_then(|playlist| playlist.last_synced_at)
                .is_some(),
            "the republished source is stamped even though the run lost a track"
        );
    });
}

/// The error→outcome mapping only. It calls `record_rejected_start` directly,
/// so it does *not* prove that any particular early return reaches it — see
/// `start_transfer_now_closes_the_open_run_when_the_device_is_gone` for the
/// call-site half, and the insufficient-space and transcode-probe tests for the
/// two rejections that can only be discovered after the log is already open.
#[test]
fn rejecting_a_start_maps_every_error_to_a_closed_run() {
    run(async {
        let (_temp, conn) = fixture();
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        let cases = [
            ("unknown device", SyncStartError::UnknownDevice),
            ("busy device", SyncStartError::Busy),
            ("scanning", SyncStartError::Planning("scanning".into())),
            ("scan error", SyncStartError::Planning("scan error".into())),
            ("read only", SyncStartError::Planning("read only".into())),
            ("blockers", SyncStartError::Planning("blockers".into())),
            (
                "insufficient space",
                SyncStartError::InsufficientSpace {
                    required_bytes: 2,
                    available_bytes: 1,
                },
            ),
            (
                "recompute delta",
                SyncStartError::Planning("recompute".into()),
            ),
            ("transcode probe", SyncStartError::Planning("probe".into())),
        ];
        for (index, (case, error)) in cases.into_iter().enumerate() {
            let log = RunLog::open(
                &runtime,
                &reprise_core::device_sync::sync_log::RunStart {
                    device_serial: "a".into(),
                    device_name: "Phone a".into(),
                    transfer_profile: "opus_160".into(),
                    started_at: 1_000 + index as i64,
                    planned: 0,
                },
            );
            record_rejected_start(&runtime, &log, &error);
            let latest = reprise_core::device_sync::sync_log::recent_runs(&conn, 1)
                .unwrap()
                .remove(0);
            let expected = if matches!(error, SyncStartError::UnknownDevice | SyncStartError::Busy)
            {
                reprise_core::device_sync::sync_log::RunOutcome::Cancelled
            } else {
                reprise_core::device_sync::sync_log::RunOutcome::Failed
            };
            assert_eq!(latest.outcome, expected, "{case}");
            assert!(
                latest.finished_at.is_some(),
                "{case} must not leave a running row"
            );
        }
    });
}

/// `start_transfer_now` must close an already-open run when its device has
/// disappeared. Assert the recorded outcome, not merely that it did not panic.
#[test]
fn start_transfer_now_closes_the_open_run_when_the_device_is_gone() {
    run(async {
        let (_temp, conn) = fixture();
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        let log = RunLog::open(
            &runtime,
            &reprise_core::device_sync::sync_log::RunStart {
                device_serial: "a".into(),
                device_name: "Phone a".into(),
                transfer_profile: "opus_160".into(),
                started_at: 5_000,
                planned: 0,
            },
        );
        // "b" is not connected — the same shape as a phone unplugged after
        // the run log opened.
        let error = runtime
            .start_transfer_now("b", SyncInitiator::Listener, log)
            .expect_err("an absent device must not start a transfer");
        assert!(matches!(error, SyncStartError::UnknownDevice));

        let latest = reprise_core::device_sync::sync_log::recent_runs(&conn, 1)
            .unwrap()
            .remove(0);
        assert_eq!(
            latest.outcome,
            reprise_core::device_sync::sync_log::RunOutcome::Cancelled,
            "the open run must be closed, not abandoned"
        );
        assert!(latest.finished_at.is_some(), "no dangling running row");
    });
}

#[test]
fn an_unrememberable_device_rejected_start_is_recorded() {
    run(async {
        let (_temp, conn) = fixture();
        let backend = Rc::new(FakeBackend::new(
            vec![descriptor("mtp://[usb:001,013]/", false)],
            1,
        ));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        gtk4::glib::timeout_future(Duration::from_millis(2)).await;

        let log = RunLog::open(&runtime, &unremembered_start(5_000));
        let error = runtime
            .start_transfer_now("missing", SyncInitiator::Listener, log)
            .expect_err("an absent device must reject the prepared run");
        assert!(matches!(error, SyncStartError::UnknownDevice));

        let recorded = reprise_core::device_sync::sync_log::recent_runs(&conn, 1)
            .unwrap()
            .remove(0);
        assert_eq!(recorded.device_serial, "mtp://[usb:001,013]/");
        assert_eq!(
            recorded.outcome,
            reprise_core::device_sync::sync_log::RunOutcome::Cancelled
        );
        assert!(recorded.finished_at.is_some());
    });
}
