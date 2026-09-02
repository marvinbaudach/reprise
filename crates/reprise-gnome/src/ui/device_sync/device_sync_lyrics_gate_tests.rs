use super::super::*;

#[test]
fn a_second_sync_with_unchanged_lyrics_does_not_replace_the_sidecar_again() {
    run(async {
        let (temp, conn) = fixture();
        std::fs::write(temp.path().join("1.lrc"), b"[00:01.00]Lyrics\n").unwrap();
        select_road_playlist(&conn, &[1]);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        runtime.sync_now("a").unwrap();
        let total = match runtime.devices()[0].sync_phase {
            PlannedSyncPhase::Syncing { total, .. } => total,
            ref phase => panic!("the run must expose its work total, got {phase:?}"),
        };
        assert_eq!(
            total, 4,
            "audio, analysis, lyrics, and playlist are four work units"
        );
        settle().await;
        std::fs::write(temp.path().join("1.flac"), vec![1_u8; 101]).unwrap();
        runtime.refresh_contents("a");
        settle().await;
        runtime.sync_now("a").unwrap();
        settle().await;

        let lyrics_copies = backend
            .state
            .managed_copies
            .borrow()
            .iter()
            .filter(|(_, path)| path.ends_with(".lrc"))
            .count();
        assert_eq!(lyrics_copies, 1);
    });
}
