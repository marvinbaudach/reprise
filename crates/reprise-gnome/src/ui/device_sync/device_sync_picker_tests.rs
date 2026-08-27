use super::*;

#[test]
fn unplugged_picker_lists_saved_sources_and_persists_a_toggle() {
    run(async {
        let (_downloads, conn) = fixture();
        crate::test_db::connection(&conn)
            .execute(
                "INSERT INTO playlists (id, name, position) VALUES (2, 'Road', 0)",
                [],
            )
            .unwrap();
        crate::test_db::connection(&conn)
            .execute(
                "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (2, 1, 0)",
                [],
            )
            .unwrap();
        let mut settings = reprise_core::device_sync::settings::load_or_create_settings(
            &conn,
            "remembered",
            "Remembered phone",
        )
        .unwrap();
        settings.selection = DeviceSelection::Sources(vec![
            SelectionSource::Playlist(2),
            SelectionSource::Smart(2),
        ]);
        settings.sync_automatically = false;
        save_settings(&conn, &settings).unwrap();

        let runtime =
            DeviceSyncRuntime::with_backend(&conn, Rc::new(FakeBackend::new(Vec::new(), 1)));
        let first = runtime.picker_snapshot("remembered").unwrap();
        assert!(first.rows.iter().any(|row| {
            row.source == SelectionSource::Playlist(2) && row.name == "Road" && row.selected
        }));
        assert!(first
            .rows
            .iter()
            .any(|row| row.source == SelectionSource::Smart(2) && row.selected));

        runtime
            .save_picker(
                "remembered",
                PickerSave {
                    playlist_changes: vec![(SelectionSource::Playlist(2), false)],
                    ..Default::default()
                },
            )
            .unwrap();
        drop(runtime);

        let reopened =
            DeviceSyncRuntime::with_backend(&conn, Rc::new(FakeBackend::new(Vec::new(), 1)));
        let saved = reopened.picker_snapshot("remembered").unwrap();
        assert!(saved.rows.iter().any(|row| {
            row.source == SelectionSource::Playlist(2) && row.name == "Road" && !row.selected
        }));
        assert!(saved
            .rows
            .iter()
            .any(|row| row.source == SelectionSource::Smart(2) && row.selected));
        assert_eq!(
            reprise_core::device_sync::settings::load_or_create_settings(
                &conn,
                "remembered",
                "Remembered phone",
            )
            .unwrap()
            .selection,
            DeviceSelection::Sources(vec![SelectionSource::Smart(2)])
        );
    });
}

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
