use super::*;

#[test]
fn smart_playlist_toggle_freezes_and_then_resumes_live_evaluation() {
    run(async {
        let (_downloads, conn) = fixture();
        let smart_id = reprise_core::library::playlists::create_smart(
            &conn,
            "All tracks",
            "[]",
            "title",
            "asc",
            None,
        )
        .unwrap();
        let source = SelectionSource::Smart(smart_id);
        disable_auto_start(&conn, "a");

        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend);
        settle().await;
        runtime
            .set_playlist_selected("a", source.clone(), true)
            .unwrap();
        runtime
            .save_picker(
                "a",
                PickerSave {
                    keep_smart_updated: Some(false),
                    ..Default::default()
                },
            )
            .unwrap();

        crate::test_db::connection(&conn)
            .execute(
                "UPDATE smart_playlists SET limit_count = 1 WHERE id = ?1",
                [smart_id],
            )
            .unwrap();
        runtime.recompute_delta("a").unwrap();
        assert_eq!(
            runtime.devices().remove(0).page.unique_track_count,
            4,
            "switching updates off keeps the originally evaluated smart-playlist copy"
        );

        reprise_core::library::settings::set_bool(&conn, KEEP_SMART_UPDATED_KEY, true).unwrap();
        runtime.recompute_delta("a").unwrap();
        assert_eq!(
            runtime.devices().remove(0).page.unique_track_count,
            1,
            "switching updates back on resumes the smart playlist's live evaluation"
        );
    });
}
