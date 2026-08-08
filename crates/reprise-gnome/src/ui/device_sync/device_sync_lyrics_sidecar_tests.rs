//! LYR-7 device-sync coverage for synchronized-lyrics sidecars.

use super::*;

#[test]
fn lyr_7_device_sync_copies_the_lrc_as_an_unmetered_track_attachment() {
    run(async {
        let (temp, conn) = fixture();
        std::fs::write(temp.path().join("1.lrc"), b"[00:01.00]Lyrics\n").unwrap();
        select_road_playlist(&conn, &[1]);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        runtime.sync_now("a").unwrap();
        settle().await;

        let copies = backend.state.managed_copies.borrow().clone();
        assert!(copies.contains(&(
            "/Music/Reprise".to_string(),
            "Artist/Unknown Album/00 Track 1.opus".to_string()
        )));
        assert!(copies.contains(&(
            "/Music/Reprise".to_string(),
            "Artist/Unknown Album/00 Track 1.lrc".to_string()
        )));
        let recorded = reprise_core::device_sync::sync_log::recent_runs(&conn, 1)
            .unwrap()
            .remove(0);
        assert_eq!(
            recorded.copied, 1,
            "the sidecar is an attachment and must not count as another copied track"
        );
    });
}

#[test]
fn lyr_7_device_sync_without_an_lrc_copies_only_the_audio() {
    run(async {
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1]);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        runtime.sync_now("a").unwrap();
        settle().await;

        assert_eq!(backend.state.managed_copies.borrow().len(), 1);
        assert!(backend.state.managed_copies.borrow()[0]
            .1
            .ends_with("Track 1.opus"));
    });
}

#[test]
fn lyr_7_removing_a_track_removes_its_lrc_without_an_extra_log_entry() {
    run(async {
        let (temp, conn) = fixture();
        std::fs::write(temp.path().join("1.lrc"), b"[00:01.00]Lyrics\n").unwrap();
        select_road_playlist(&conn, &[1]);
        let first_backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let first_runtime = DeviceSyncRuntime::with_backend(&conn, first_backend);
        settle().await;
        first_runtime.sync_now("a").unwrap();
        settle().await;
        drop(first_runtime);

        crate::test_db::connection(&conn)
            .execute("DELETE FROM playlist_tracks WHERE playlist_id = 10", [])
            .unwrap();
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;
        runtime.sync_now("a").unwrap();
        settle().await;

        let deleted = backend.state.managed_deleted.borrow().clone();
        assert!(deleted
            .iter()
            .any(|(_, path)| path.ends_with("Track 1.opus")));
        assert!(deleted
            .iter()
            .any(|(_, path)| path.ends_with("Track 1.lrc")));
        let recorded = reprise_core::device_sync::sync_log::recent_runs(&conn, 1)
            .unwrap()
            .remove(0);
        assert_eq!(recorded.deleted, 1);
    });
}

#[test]
fn lyr_7_removing_a_track_leaves_an_lrc_reprise_never_mirrored_alone() {
    run(async {
        // No `.lrc` in the library: whatever sits beside the audio on the
        // device is the user's own, hand-authored on a player with no
        // internet, and may be the only copy in existence.
        let (_temp, conn) = fixture();
        select_road_playlist(&conn, &[1]);
        let first_backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let first_runtime = DeviceSyncRuntime::with_backend(&conn, first_backend);
        settle().await;
        first_runtime.sync_now("a").unwrap();
        settle().await;
        drop(first_runtime);

        crate::test_db::connection(&conn)
            .execute("DELETE FROM playlist_tracks WHERE playlist_id = 10", [])
            .unwrap();
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;
        runtime.sync_now("a").unwrap();
        settle().await;

        let deleted = backend.state.managed_deleted.borrow().clone();
        assert!(deleted
            .iter()
            .any(|(_, path)| path.ends_with("Track 1.opus")));
        assert!(
            !deleted.iter().any(|(_, path)| path.ends_with(".lrc")),
            "no library sidecar means Reprise never put one there, got {deleted:?}"
        );
    });
}

#[test]
fn lyr_7_a_failed_lrc_copy_never_fails_the_track_transfer() {
    run(async {
        let (temp, conn) = fixture();
        std::fs::write(temp.path().join("1.lrc"), b"[00:01.00]Lyrics\n").unwrap();
        select_road_playlist(&conn, &[1]);
        let backend = Rc::new(
            FakeBackend::new(vec![descriptor("a", true)], 1)
                .with_sidecar_replace_error("simulated sidecar refusal"),
        );
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        runtime.sync_now("a").unwrap();
        settle().await;

        let device = runtime.devices().remove(0);
        assert!(device.last_sync.is_some());
        let recorded = reprise_core::device_sync::sync_log::recent_runs(&conn, 1)
            .unwrap()
            .remove(0);
        assert_eq!(recorded.copied, 1);
        assert_eq!(
            reprise_core::device_sync::settings::load_device_files(&conn, "a")
                .unwrap()
                .len(),
            1
        );
        assert_eq!(backend.state.managed_copies.borrow().len(), 1);
    });
}

#[test]
fn lyr_7_a_profile_replacement_keeps_the_shared_lrc_basename() {
    run(async {
        let (temp, conn) = fixture();
        let source = temp.path().join("1.flac");
        std::fs::write(temp.path().join("1.lrc"), b"[00:01.00]Lyrics\n").unwrap();
        select_road_playlist(&conn, &[1]);
        reprise_core::device_sync::settings::upsert_device_file(
            &conn,
            &reprise_core::device_sync::DeviceFileRecord {
                device_serial: "a".into(),
                track_id: 1,
                source_path: source.to_string_lossy().into_owned(),
                source_size: 100,
                source_mtime: 0,
                device_path: "Artist/Unknown Album/00 Track 1.flac".into(),
                device_size: 100,
                profile_fingerprint: "copy-original-v1".into(),
                pinned: false,
            },
        )
        .unwrap();
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        runtime.sync_now("a").unwrap();
        settle().await;

        assert!(backend
            .state
            .managed_copies
            .borrow()
            .iter()
            .any(|(_, path)| path.ends_with("Track 1.lrc")));
        assert!(!backend
            .state
            .managed_deleted
            .borrow()
            .iter()
            .any(|(_, path)| path.ends_with("Track 1.lrc")));
    });
}
