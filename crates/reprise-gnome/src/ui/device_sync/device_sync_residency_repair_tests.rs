use super::*;
use reprise_core::device_sync::settings::{upsert_device_file, DeviceFileRecord};
use reprise_core::spectrogram::{TrackSourceFingerprint, TrackSpectrogram};
use reprise_core::waveform::TrackRenderData;

const DEVICE_PATH: &str = "Artist/Unknown Album/00 Track 1.opus";

impl FakeBackend {
    fn set_probe_result(&self, result: Result<Vec<ManagedDeviceFile>, String>) {
        self.state.probe_result.replace(Some(result));
    }

    fn probe_call_count(&self) -> usize {
        self.state.probe_calls.get()
    }
}

fn seed_selected_inventory(conn: &Rc<Db>, temp: &tempfile::TempDir) {
    select_road_playlist(conn, &[1]);
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
            device_path: DEVICE_PATH.into(),
            device_size: 100,
            profile_fingerprint: "opus-vbr-160-v1".into(),
            pinned: false,
        },
    )
    .unwrap();
}

fn seed_analysis(conn: &Db) {
    reprise_core::db::set_track_render_data(
        conn,
        1,
        TrackSourceFingerprint {
            mtime_seconds: 0,
            size_bytes: 0,
            device: None,
            inode: None,
        },
        &TrackRenderData {
            waveform_peaks: vec![3; 4],
            spectrogram: TrackSpectrogram::from_cells(vec![7; 24]).unwrap(),
        },
    )
    .unwrap();
}

#[test]
fn short_walk_recovers_present_track_with_device_reported_size() {
    run(async {
        let (temp, conn) = fixture();
        seed_selected_inventory(&conn, &temp);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        backend.set_probe_result(Ok(vec![ManagedDeviceFile {
            relative_path: DEVICE_PATH.into(),
            size_bytes: 321,
        }]));

        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        let device = runtime.devices().remove(0);
        assert_eq!(device.page.changes.additions, 0);
        assert_eq!(device.content_row.item_count, 1);
        assert_eq!(device.content_row.size_on_device_bytes, 321);
        assert_eq!(
            device.memory_status.as_deref(),
            Some("Scan was incomplete — 1 file re-checked; 1 recovered")
        );
        assert_eq!(backend.probe_call_count(), 1);
    });
}

#[test]
fn hard_probe_error_disarms_absence_without_reporting_a_scan_error() {
    run(async {
        let (temp, conn) = fixture();
        seed_selected_inventory(&conn, &temp);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        backend.set_probe_result(Err("phone vanished".into()));

        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        settle().await;

        let device = runtime.devices().remove(0);
        assert!(device.scan_error.is_none());
        assert_eq!(device.page.changes.additions, 0);
    });
}

#[test]
fn short_walk_recovers_analysis_sidecar_without_rewriting_it() {
    run(async {
        let (temp, conn) = fixture();
        seed_selected_inventory(&conn, &temp);
        seed_analysis(&conn);
        let sidecar_path =
            reprise_core::device_sync::analysis_sidecar::device_path_for_track(DEVICE_PATH)
                .unwrap();
        let sidecar_size =
            reprise_core::device_sync::analysis_sidecar::AnalysisSidecar::for_track(&conn, 1)
                .unwrap()
                .unwrap()
                .encode()
                .unwrap()
                .len() as u64;
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        backend.set_probe_result(Ok(vec![
            ManagedDeviceFile {
                relative_path: DEVICE_PATH.into(),
                size_bytes: 100,
            },
            ManagedDeviceFile {
                relative_path: sidecar_path,
                size_bytes: sidecar_size,
            },
        ]));

        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        settle().await;

        let device = runtime.devices().remove(0);
        assert_eq!(device.page.changes.additions, 0);
        assert_eq!(
            device.memory_status.as_deref(),
            Some("Scan was incomplete — 2 files re-checked; 2 recovered")
        );
    });
}

#[test]
fn short_walk_recovers_independently_missing_sidecar_without_duplicate_audio() {
    run(async {
        let (temp, conn) = fixture();
        seed_selected_inventory(&conn, &temp);
        seed_analysis(&conn);
        let sidecar_path =
            reprise_core::device_sync::analysis_sidecar::device_path_for_track(DEVICE_PATH)
                .unwrap();
        let sidecar_size =
            reprise_core::device_sync::analysis_sidecar::AnalysisSidecar::for_track(&conn, 1)
                .unwrap()
                .unwrap()
                .encode()
                .unwrap()
                .len() as u64;
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        backend.state.managed_files.replace(vec![ManagedDeviceFile {
            relative_path: DEVICE_PATH.into(),
            size_bytes: 100,
        }]);
        backend.set_probe_result(Ok(vec![ManagedDeviceFile {
            relative_path: sidecar_path,
            size_bytes: sidecar_size,
        }]));

        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        let device = runtime.devices().remove(0);
        assert_eq!(device.page.changes.additions, 0);
        assert_eq!(device.content_row.item_count, 2);
        assert_eq!(backend.probe_call_count(), 1);
    });
}

#[test]
fn short_walk_keeps_genuinely_absent_track_planned_for_copy() {
    run(async {
        let (temp, conn) = fixture();
        seed_selected_inventory(&conn, &temp);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        backend.set_probe_result(Ok(Vec::new()));

        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        assert_eq!(runtime.devices()[0].page.changes.additions, 1);
        assert_eq!(backend.probe_call_count(), 1);
    });
}

#[test]
fn complete_walk_skips_probe_and_arms_residency_proof() {
    run(async {
        let (temp, conn) = fixture();
        seed_selected_inventory(&conn, &temp);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        backend.state.managed_files.replace(vec![
            ManagedDeviceFile {
                relative_path: DEVICE_PATH.into(),
                size_bytes: 100,
            },
            ManagedDeviceFile {
                relative_path: reprise_core::device_sync::analysis_sidecar::device_path_for_track(
                    DEVICE_PATH,
                )
                .unwrap(),
                size_bytes: 1,
            },
        ]);

        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        let mut states = runtime.device_states.borrow_mut();
        assert_eq!(backend.probe_call_count(), 0);
        assert!(states[0].managed_files_scanned());
        states[0].residency_proven = false;
        assert!(!states[0].managed_files_scanned());
        drop(states);
        assert_eq!(runtime.devices()[0].page.changes.additions, 0);
    });
}

#[test]
fn clean_absence_after_a_repair_clears_the_short_scan_message() {
    run(async {
        let (temp, conn) = fixture();
        seed_selected_inventory(&conn, &temp);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        backend.set_probe_result(Ok(vec![ManagedDeviceFile {
            relative_path: DEVICE_PATH.into(),
            size_bytes: 100,
        }]));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;
        assert!(runtime.devices()[0].memory_status.is_some());

        backend.set_probe_result(Ok(Vec::new()));
        runtime.refresh_contents("a");
        settle().await;

        assert!(runtime.devices()[0].memory_status.is_none());
    });
}

#[test]
fn disconnect_clears_residency_repair_state_before_reconnect_refresh() {
    run(async {
        let (temp, conn) = fixture();
        seed_selected_inventory(&conn, &temp);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        backend.set_probe_result(Ok(vec![ManagedDeviceFile {
            relative_path: DEVICE_PATH.into(),
            size_bytes: 100,
        }]));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;
        assert!(runtime.devices()[0].memory_status.is_some());

        backend.set_devices(&[]);
        // This pins the disconnect reset: disconnected projections read
        // `short_scan` without consulting `ever_inspected`.
        assert!(runtime.devices()[0].memory_status.is_none());
    });
}

#[test]
fn missing_target_folder_skips_path_probes_and_disarms_absence() {
    run(async {
        let (temp, conn) = fixture();
        seed_selected_inventory(&conn, &temp);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        backend.set_managed_target_exists(false);

        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        let device = runtime.devices().remove(0);
        assert_eq!(backend.probe_call_count(), 0);
        assert_eq!(device.page.changes.additions, 0);
        assert!(device.scan_error.is_none());
    });
}
