//! `MTP-30` (design 7a, turn-6 plan E5) wiring: the pure decision itself is
//! unit-tested in `reprise_core::device_sync::auto_start`; these tests only
//! prove the GTK runtime gathers the right facts and obeys it — connecting a
//! device with the switch on and real work pending starts a sync with no
//! button pressed, and every other case stays silent.

use super::*;

/// Seeds playlist `10` with `ids` selected for device `"a"` and sets
/// `sync_automatically` explicitly, unlike `select_road_playlist` (which
/// always turns it on) — needed here to exercise the switch being off.
fn seed_playlist_with_auto_start(
    conn: &Rc<RefCell<Connection>>,
    ids: &[i64],
    sync_automatically: bool,
) {
    conn.borrow()
        .execute(
            "INSERT INTO playlists (id, name, position) VALUES (10, 'Road', 0)",
            [],
        )
        .unwrap();
    for (position, track_id) in ids.iter().enumerate() {
        conn.borrow()
            .execute(
                "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (10, ?1, ?2)",
                rusqlite::params![track_id, position as i64],
            )
            .unwrap();
    }
    save_settings(
        &conn.borrow(),
        &DeviceSettings {
            device_serial: "a".into(),
            device_name: "Phone a".into(),
            selection: DeviceSelection::Sources(vec![SelectionSource::Playlist(10)]),
            profile: reprise_core::device_sync::TransferProfile::default(),
            opus_bitrate: 0,
            ratings_back: false,
            remove_deleted: true,
            sync_automatically,
        },
    )
    .unwrap();
}

#[test]
fn mtp_30_auto_starts_on_connect_when_the_switch_is_on_and_work_is_pending() {
    run(async {
        let (_temp, conn) = fixture();
        seed_playlist_with_auto_start(&conn, &[1], true);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        assert_eq!(
            backend.state.copy_order.borrow().len(),
            1,
            "the switch is on and a track is pending: connect alone must start the sync"
        );
        let device = runtime.devices().remove(0);
        assert!(device.last_sync.is_some());
        assert_eq!(device.sync_phase, PlannedSyncPhase::Idle);
    });
}

#[test]
fn mtp_30_stays_silent_when_the_switch_is_off() {
    run(async {
        let (_temp, conn) = fixture();
        seed_playlist_with_auto_start(&conn, &[1], false);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        assert!(
            backend.state.copy_order.borrow().is_empty(),
            "the switch is off: connect must never start a sync by itself"
        );
        let device = runtime.devices().remove(0);
        assert!(device.last_sync.is_none());
        assert_eq!(
            device.page.changes.additions, 1,
            "the plan itself is unaffected — only the automatic start is suppressed"
        );
    });
}

#[test]
fn mtp_30_stays_silent_when_the_connect_scan_fails() {
    run(async {
        let (_temp, conn) = fixture();
        seed_playlist_with_auto_start(&conn, &[1], true);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        backend.fail_next_inspection("phone was locked");
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        assert!(
            backend.state.copy_order.borrow().is_empty(),
            "an unverified scan must never be trusted enough to start a sync automatically"
        );
        let device = runtime.devices().remove(0);
        assert_eq!(device.scan_error.as_deref(), Some("phone was locked"));
        assert!(device.last_sync.is_none());
    });
}

/// `MTP-41`/`MTP-30`: before `MTP-41`'s live wiring fix,
/// `files_waiting_for_download` was hard-coded to `0`, so a device with
/// only a wanted-but-missing podcast episode (`wanted_on_device`, `MTP-40`)
/// pending — nothing to copy, nothing to remove — produced an all-zero
/// `SyncBalance` that read as "nothing to do". `should_auto_start` (and,
/// downstream, the sidebar's "Up to date" reading, `MTP-29`) must instead
/// see this as real pending work.
#[test]
fn mtp_30_a_waiting_only_podcast_balance_would_still_trigger_automatic_start() {
    run(async {
        let (_downloads, conn) = fixture();
        conn.borrow()
            .execute(
                "INSERT INTO podcast_subscriptions
                 (id, kind, feed_url, title, auto_download, sync_to_phone, added_at)
                 VALUES (10, 'rss', 'https://example.test/rss', 'RSS Show', 0, 1, 1)",
                [],
            )
            .unwrap();
        conn.borrow()
            .execute(
                "INSERT INTO podcast_subscription_devices (subscription_id, device_id)
                 VALUES (10, 'a')",
                [],
            )
            .unwrap();
        conn.borrow()
            .execute(
                "INSERT INTO podcast_episodes
                 (id, subscription_id, guid, title, audio_url, wanted_on_device, first_seen_at)
                 VALUES (100, 10, 'rss-100', 'Wanted', 'https://example.test/w.mp3', 1, 1)",
                [],
            )
            .unwrap();
        disable_auto_start(&conn, "a");

        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;

        let device = runtime.devices().remove(0);
        let balance = reprise_core::device_sync::aggregate_balance(&device.category_readings);
        assert_eq!(balance.files_to_copy, 0, "nothing is downloaded yet");
        assert_eq!(
            balance.files_waiting_for_download, 1,
            "the wanted-but-missing episode must reach the balance, not vanish from it"
        );

        let facts = reprise_core::device_sync::AutoStartFacts {
            just_connected: true,
            sync_automatically: true,
            scan_ok: device.scan_error.is_none(),
            planning_ok: true,
            device_connected: device.connected,
            device_busy: false,
            balance,
        };
        assert!(
            reprise_core::device_sync::should_auto_start(facts),
            "a waiting episode is pending work — before this fix, an all-zero balance would \
             have made `should_auto_start` refuse and the device would read 'Up to date' with \
             work genuinely pending"
        );
    });
}

#[test]
fn mtp_30_a_manual_refresh_never_retriggers_it() {
    run(async {
        let (_temp, conn) = fixture();
        // Connects with the switch off, so nothing starts on connect.
        seed_playlist_with_auto_start(&conn, &[1], false);
        let backend = Rc::new(FakeBackend::new(vec![descriptor("a", true)], 1));
        let runtime = DeviceSyncRuntime::with_backend(&conn, backend.clone());
        settle().await;
        assert!(backend.state.copy_order.borrow().is_empty());

        // Flip the switch on with work still pending, then trigger the
        // user's manual "Refresh" action — the same conditions that would
        // auto-start on a fresh connect must NOT auto-start here, because
        // this is not the first refresh after a connect.
        runtime
            .update_settings(DeviceSettings {
                device_serial: "a".into(),
                device_name: "Phone a".into(),
                selection: DeviceSelection::Sources(vec![SelectionSource::Playlist(10)]),
                profile: reprise_core::device_sync::TransferProfile::default(),
                opus_bitrate: 0,
                ratings_back: false,
                remove_deleted: true,
                sync_automatically: true,
            })
            .unwrap();
        runtime.refresh_contents("a");
        settle().await;

        assert!(
            backend.state.copy_order.borrow().is_empty(),
            "a manual refresh must never start a sync by itself, even once every other condition holds"
        );
        assert!(runtime.devices().remove(0).last_sync.is_none());
    });
}
