//! Desktop consumption of the phone's `RPT-BACK` journal during one sync run.

use super::*;
use reprise_core::device_sync::listen_report::{
    ListenEntry, ListenReport, ListenReportAcknowledgement, RatingEntry, ACKNOWLEDGEMENT_FILE_NAME,
    REPORT_FILE_NAME,
};
use reprise_core::device_sync::settings::{upsert_device_file, DeviceFileRecord};
use reprise_core::device_sync::sync_log::{DeviationKind, RunOutcome};
use reprise_core::device_sync::track_metadata_list::{
    TrackMetadataList, FILE_NAME as TRACK_METADATA_FILE_NAME,
};

const TRACK_PATH: &str = "Artist/Unknown Album/00 Track 1.opus";

fn report(listens: Vec<ListenEntry>, ratings: Vec<RatingEntry>) -> Vec<u8> {
    ListenReport::new(listens, ratings).encode().unwrap()
}

fn listen(sequence: u64, path: &str) -> ListenEntry {
    ListenEntry {
        sequence,
        device_path: path.into(),
        played_at: 1_754_600_100,
        ms_played: 900,
    }
}

fn rating(sequence: u64, path: &str) -> RatingEntry {
    RatingEntry {
        sequence,
        device_path: path.into(),
        rating: 5,
        rated_at: 20,
    }
}

fn register_synced_track(conn: &Db, temp: &tempfile::TempDir) -> ManagedDeviceFile {
    let source_path = temp.path().join("1.flac");
    let metadata = std::fs::metadata(&source_path).unwrap();
    let source_mtime = metadata
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    upsert_device_file(
        conn,
        &DeviceFileRecord {
            device_serial: "a".into(),
            track_id: 1,
            source_path: source_path.to_string_lossy().into_owned(),
            source_size: metadata.len(),
            source_mtime,
            device_path: TRACK_PATH.into(),
            device_size: 100,
            profile_fingerprint: "opus-vbr-160-v1".into(),
            pinned: false,
        },
    )
    .unwrap();
    ManagedDeviceFile {
        relative_path: TRACK_PATH.into(),
        size_bytes: 100,
    }
}

fn copied_file(backend: &FakeBackend, name: &str) -> Option<Vec<u8>> {
    backend
        .state
        .managed_copy_contents
        .borrow()
        .iter()
        .find(|(_, path, _)| path == name)
        .map(|(_, _, bytes)| bytes.clone())
}

#[test]
fn desktop_run_applies_report_before_outbound_metadata_and_records_every_result() {
    run(async {
        let (temp, conn) = fixture();
        select_road_playlist(&conn, &[1]);
        crate::test_db::connection(&conn)
            .execute(
                "UPDATE tracks SET rating = 2, rated_at = 10, play_count = 3 WHERE id = 1",
                [],
            )
            .unwrap();
        let synced_track = register_synced_track(&conn, &temp);
        let bytes = report(
            vec![listen(11, TRACK_PATH), listen(13, "Gone/Listen.opus")],
            vec![rating(12, TRACK_PATH), rating(14, "Gone/Rating.opus")],
        );
        let backend =
            Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1).with_listen_report(bytes));
        backend.state.managed_files.replace(vec![synced_track]);
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        runtime.sync_now("a").unwrap();
        settle().await;

        assert_eq!(
            crate::test_db::connection(&conn)
                .query_row(
                    "SELECT play_count, rating FROM tracks WHERE id = 1",
                    [],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i32>(1)?)),
                )
                .unwrap(),
            (4, 5)
        );
        let metadata = TrackMetadataList::decode(
            &copied_file(&backend, TRACK_METADATA_FILE_NAME).expect("outbound metadata"),
        )
        .unwrap();
        assert_eq!(
            (metadata.entries[0].play_count, metadata.entries[0].rating),
            (4, 5),
            "the returned mutations must be visible before the outbound list is encoded"
        );
        let acknowledgement = ListenReportAcknowledgement::decode(
            &copied_file(&backend, ACKNOWLEDGEMENT_FILE_NAME).expect("acknowledgement"),
        )
        .unwrap();
        assert_eq!(acknowledgement.applied_sequence, 14);

        let recorded = reprise_core::device_sync::sync_log::recent_runs(&conn, 1)
            .unwrap()
            .remove(0);
        assert_eq!(recorded.outcome, RunOutcome::Completed);
        assert_eq!(recorded.listens_applied, 1);
        assert_eq!(recorded.ratings_applied, 1);
        assert_eq!(recorded.skipped, 2);
        let deviations =
            reprise_core::device_sync::sync_log::deviations(&conn, recorded.id).unwrap();
        assert_eq!(
            deviations
                .iter()
                .map(|deviation| (
                    deviation.kind,
                    deviation.device_path.as_str(),
                    deviation.detail.as_str()
                ))
                .collect::<Vec<_>>(),
            [
                (
                    DeviationKind::Skipped,
                    "Gone/Listen.opus",
                    "phone listen report path could not be resolved"
                ),
                (
                    DeviationKind::Skipped,
                    "Gone/Rating.opus",
                    "phone listen report path could not be resolved"
                ),
            ]
        );
    });
}

#[test]
fn automatic_connect_sync_imports_and_acknowledges_without_publishing_judgements() {
    run(async {
        let (temp, conn) = fixture();
        select_road_playlist(&conn, &[1]);
        let _synced_track = register_synced_track(&conn, &temp);
        let mut settings =
            reprise_core::device_sync::settings::load_or_create_settings(&conn, "a", "Phone a")
                .unwrap();
        settings.sync_automatically = true;
        save_settings(&conn, &settings).unwrap();
        let backend = Rc::new(
            FakeBackend::new(vec![descriptor("a", true)], 1)
                .with_listen_report(report(vec![listen(21, TRACK_PATH)], Vec::new())),
        );
        // Leaving the corresponding managed audio absent gives the automatic
        // run real work while the inventory row still resolves the report.
        backend.state.managed_files.replace(Vec::new());

        let _runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        assert_eq!(
            crate::test_db::connection(&conn)
                .query_row("SELECT play_count FROM tracks WHERE id = 1", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            1
        );
        assert!(copied_file(&backend, ACKNOWLEDGEMENT_FILE_NAME).is_some());
        assert_eq!(
            copied_file(&backend, TRACK_METADATA_FILE_NAME),
            None,
            "MTP-30 still forbids cable-triggered judgement publication"
        );
        let recorded = reprise_core::device_sync::sync_log::recent_runs(&conn, 1)
            .unwrap()
            .remove(0);
        assert_eq!(recorded.listens_applied, 1);
    });
}

#[test]
fn failed_report_apply_never_writes_an_acknowledgement_or_breaks_the_sync() {
    run(async {
        let (temp, conn) = fixture();
        select_road_playlist(&conn, &[1]);
        let synced_track = register_synced_track(&conn, &temp);
        crate::test_db::connection(&conn)
            .execute_batch(
                "CREATE TRIGGER reject_returned_listen
                 BEFORE INSERT ON listen_events
                 BEGIN SELECT RAISE(ABORT, 'reject returned listen'); END;",
            )
            .unwrap();
        let backend = Rc::new(
            FakeBackend::new(vec![descriptor("a", true)], 1)
                .with_listen_report(report(vec![listen(31, TRACK_PATH)], Vec::new())),
        );
        backend.state.managed_files.replace(vec![synced_track]);
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        runtime.sync_now("a").unwrap();
        settle().await;

        assert_eq!(copied_file(&backend, ACKNOWLEDGEMENT_FILE_NAME), None);
        assert!(copied_file(&backend, TRACK_METADATA_FILE_NAME).is_some());
        let recorded = reprise_core::device_sync::sync_log::recent_runs(&conn, 1)
            .unwrap()
            .remove(0);
        assert_eq!(recorded.outcome, RunOutcome::Completed);
        assert_eq!(recorded.listens_applied, 0);
    });
}

#[test]
fn absent_unreadable_and_malformed_reports_leave_the_existing_sync_intact() {
    run(async {
        for broken in [
            None,
            Some(Ok(b"not-rpt-back".to_vec())),
            Some(Err("read refused")),
        ] {
            let (temp, conn) = fixture();
            select_road_playlist(&conn, &[1]);
            let synced_track = register_synced_track(&conn, &temp);
            let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
            match broken {
                None => {}
                Some(Ok(bytes)) => backend.set_listen_report(bytes),
                Some(Err(error)) => backend.set_listen_report_read_error(error),
            }
            backend.state.managed_files.replace(vec![synced_track]);
            let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
            settle().await;

            runtime.sync_now("a").unwrap();
            settle().await;

            assert!(copied_file(&backend, TRACK_METADATA_FILE_NAME).is_some());
            assert_eq!(copied_file(&backend, ACKNOWLEDGEMENT_FILE_NAME), None);
            assert_eq!(
                reprise_core::device_sync::sync_log::recent_runs(&conn, 1)
                    .unwrap()
                    .remove(0)
                    .outcome,
                RunOutcome::Completed
            );
            assert!(backend
                .state
                .managed_reads
                .borrow()
                .contains(&("/Music/Reprise".into(), REPORT_FILE_NAME.into())));
        }
    });
}

#[test]
fn a_session_only_device_never_acknowledges_actions_it_cannot_resolve_durably() {
    run(async {
        let (_temp, conn) = fixture();
        let device_id = "mtp://[usb:001,013]/";
        let backend = Rc::new(
            FakeBackend::new(vec![descriptor(device_id, false)], 1)
                .with_listen_report(report(vec![listen(41, TRACK_PATH)], Vec::new())),
        );
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;
        let mut settings = runtime.devices().remove(0).settings;
        settings.selection = DeviceSelection::EntireLibrary;
        runtime.update_settings(settings).unwrap();

        runtime.sync_now(device_id).unwrap();
        settle().await;

        assert!(backend
            .state
            .managed_reads
            .borrow()
            .iter()
            .any(|(_, path)| path == REPORT_FILE_NAME));
        assert_eq!(copied_file(&backend, ACKNOWLEDGEMENT_FILE_NAME), None);
        assert_eq!(
            crate::test_db::connection(&conn)
                .query_row(
                    "SELECT COUNT(*) FROM device_listen_report_state",
                    [],
                    |row| { row.get::<_, i64>(0) }
                )
                .unwrap(),
            0,
            "a volatile URI must not become an acknowledgement identity"
        );
    });
}
