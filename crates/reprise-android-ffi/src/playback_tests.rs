use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use reprise_core::library::settings::TrackTransition;
use reprise_core::playback::{
    AudioEffects, PlaybackBackend, PlaybackState, PlayerEvent, StreamEvent, StreamGeneration,
};

use super::test_support::{recording_session, PortCall, RecordingListener, RecordingPort};
use crate::playback::{
    AndroidPlaybackBackend, AndroidPlaybackState, AndroidPlayerEvent, AndroidTransitionMode,
    PlaybackEventBridge,
};
use crate::{
    AndroidEqualizerPoint, AndroidPlaybackSession, AndroidPlaybackSnapshot, AndroidRepeatMode,
};

#[test]
fn android_backend_routes_every_core_command_through_the_media3_port() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let backend = AndroidPlaybackBackend::new(
        Box::new(RecordingPort {
            calls: Arc::clone(&calls),
            bridge: Arc::new(Mutex::new(None)),
        }),
        Box::new(|_| {}),
    )
    .unwrap();

    backend.play("/music/song.flac").unwrap();
    backend
        .play_uri("content://provider/document/song.flac")
        .unwrap();
    assert_eq!(backend.toggle_pause().unwrap(), PlaybackState::Paused);
    backend.seek_to(1_250).unwrap();
    backend.set_volume(0.4);
    let effects_error = backend
        .set_audio_effects(AudioEffects::default())
        .unwrap_err();
    let spectrum_error = backend.set_spectrum_enabled(true).unwrap_err();
    backend.stop().unwrap();
    backend.set_next(Some("content://provider/document/next.flac"));
    backend.set_next(None);
    backend.set_transition(TrackTransition::Crossfade, 8);
    assert_eq!(backend.current_generation(), StreamGeneration::from(23));

    assert_eq!(
        calls.lock().unwrap().as_slice(),
        &[
            PortCall::SetEventBridge,
            PortCall::PlayPath("/music/song.flac".to_owned()),
            PortCall::PlayUri("content://provider/document/song.flac".to_owned()),
            PortCall::TogglePause,
            PortCall::SeekTo(1_250),
            PortCall::SetVolume(0.4),
            PortCall::SetAudioEffects,
            PortCall::SetSpectrumEnabled(true),
            PortCall::Stop,
            PortCall::SetNext(Some("content://provider/document/next.flac".to_owned())),
            PortCall::SetNext(None),
            PortCall::SetTransition(AndroidTransitionMode::Gapless),
            PortCall::CurrentGeneration,
        ]
    );
    assert!(effects_error
        .to_string()
        .contains("audio effects are not supported"));
    assert!(spectrum_error
        .to_string()
        .contains("spectrum analysis is not supported"));
}

#[test]
fn tapping_a_track_starts_a_core_queue_at_that_position() {
    let fixture = recording_session();

    fixture
        .session
        .play_tracks(
            vec![10, 11, 12],
            vec![
                "content://provider/first.flac".to_owned(),
                "content://provider/second.flac".to_owned(),
                "content://provider/third.flac".to_owned(),
            ],
            1,
        )
        .unwrap();

    assert_eq!(
        fixture.session.snapshot().unwrap(),
        AndroidPlaybackSnapshot {
            state: AndroidPlaybackState::Playing,
            current_index: Some(1),
            current_track_id: Some(11),
            current_track_uri: Some("content://provider/second.flac".to_owned()),
            position_ms: 0,
            duration_ms: 0,
            automatic_advance_count: 0,
            shuffled: false,
            repeat: AndroidRepeatMode::Off,
            error: None,
        }
    );
    assert_eq!(
        fixture.calls.lock().unwrap().as_slice(),
        &[
            PortCall::SetEventBridge,
            PortCall::SetEqualizer(
                false,
                vec![
                    AndroidEqualizerPoint {
                        frequency_hz: 29.0,
                        gain_db: 0.0
                    },
                    AndroidEqualizerPoint {
                        frequency_hz: 59.0,
                        gain_db: 0.0
                    },
                    AndroidEqualizerPoint {
                        frequency_hz: 119.0,
                        gain_db: 0.0
                    },
                    AndroidEqualizerPoint {
                        frequency_hz: 237.0,
                        gain_db: 0.0
                    },
                    AndroidEqualizerPoint {
                        frequency_hz: 474.0,
                        gain_db: 0.0
                    },
                    AndroidEqualizerPoint {
                        frequency_hz: 947.0,
                        gain_db: 0.0
                    },
                    AndroidEqualizerPoint {
                        frequency_hz: 1_889.0,
                        gain_db: 0.0
                    },
                    AndroidEqualizerPoint {
                        frequency_hz: 3_770.0,
                        gain_db: 0.0
                    },
                    AndroidEqualizerPoint {
                        frequency_hz: 7_523.0,
                        gain_db: 0.0
                    },
                    AndroidEqualizerPoint {
                        frequency_hz: 15_011.0,
                        gain_db: 0.0
                    },
                ]
            ),
            PortCall::SetTransition(AndroidTransitionMode::Gapless),
            PortCall::PlayUri("content://provider/second.flac".to_owned()),
            PortCall::CurrentGeneration,
            PortCall::SetNext(Some("content://provider/third.flac".to_owned())),
        ]
    );
    assert_eq!(
        fixture.snapshots.lock().unwrap().last(),
        Some(&fixture.session.snapshot().unwrap())
    );
}

#[test]
fn an_empty_session_snapshot_has_no_current_track_identity() {
    let fixture = recording_session();

    assert_eq!(
        fixture.session.snapshot().unwrap(),
        AndroidPlaybackSnapshot {
            state: AndroidPlaybackState::Stopped,
            current_index: None,
            current_track_id: None,
            current_track_uri: None,
            position_ms: 0,
            duration_ms: 0,
            automatic_advance_count: 0,
            shuffled: false,
            repeat: AndroidRepeatMode::Off,
            error: None,
        }
    );
}

#[test]
fn a_new_selection_replaces_the_current_track_identity() {
    let fixture = recording_session();
    fixture
        .session
        .play_tracks(
            vec![10, 11],
            vec![
                "content://provider/first.flac".to_owned(),
                "content://provider/second.flac".to_owned(),
            ],
            1,
        )
        .unwrap();

    fixture
        .session
        .play_tracks(
            vec![90],
            vec!["content://provider/replacement.flac".to_owned()],
            0,
        )
        .unwrap();

    let snapshot = fixture.session.snapshot().unwrap();
    assert_eq!(snapshot.current_track_id, Some(90));
    assert_eq!(
        snapshot.current_track_uri.as_deref(),
        Some("content://provider/replacement.flac")
    );
}

#[test]
fn core_queue_owns_gapless_advance_and_manual_next_previous() {
    let fixture = recording_session();
    fixture
        .session
        .play_tracks(
            vec![10, 11, 12],
            vec![
                "content://provider/first.flac".to_owned(),
                "content://provider/second.flac".to_owned(),
                "content://provider/third.flac".to_owned(),
            ],
            0,
        )
        .unwrap();
    fixture.calls.lock().unwrap().clear();
    let bridge = fixture.bridge.lock().unwrap().clone().unwrap();

    bridge.emit(24, AndroidPlayerEvent::AdvancedToNext);
    bridge.emit(
        24,
        AndroidPlayerEvent::Position {
            position_ms: 1_250,
            duration_ms: 180_000,
        },
    );

    assert_eq!(
        fixture.session.snapshot().unwrap(),
        AndroidPlaybackSnapshot {
            state: AndroidPlaybackState::Playing,
            current_index: Some(1),
            current_track_id: Some(11),
            current_track_uri: Some("content://provider/second.flac".to_owned()),
            position_ms: 1_250,
            duration_ms: 180_000,
            automatic_advance_count: 1,
            shuffled: false,
            repeat: AndroidRepeatMode::Off,
            error: None,
        }
    );
    assert_eq!(
        fixture.calls.lock().unwrap().as_slice(),
        &[PortCall::SetNext(Some(
            "content://provider/third.flac".to_owned()
        ))]
    );

    fixture.session.next().unwrap();
    fixture.session.previous().unwrap();

    assert_eq!(fixture.session.snapshot().unwrap().current_index, Some(1));
    assert_eq!(
        fixture.calls.lock().unwrap().as_slice(),
        &[
            PortCall::SetNext(Some("content://provider/third.flac".to_owned())),
            PortCall::PlayUri("content://provider/third.flac".to_owned()),
            PortCall::CurrentGeneration,
            PortCall::SetNext(None),
            PortCall::PlayUri("content://provider/second.flac".to_owned()),
            PortCall::CurrentGeneration,
            PortCall::SetNext(Some("content://provider/third.flac".to_owned())),
        ]
    );
}

#[test]
fn snapshot_counts_automatic_advances_but_not_manual_skips() {
    let fixture = recording_session();
    fixture
        .session
        .play_tracks(
            vec![10, 11, 12],
            vec![
                "content://provider/first.flac".to_owned(),
                "content://provider/second.flac".to_owned(),
                "content://provider/third.flac".to_owned(),
            ],
            0,
        )
        .unwrap();

    assert_eq!(
        fixture.session.snapshot().unwrap().automatic_advance_count,
        0
    );
    fixture.session.next().unwrap();
    fixture.session.previous().unwrap();
    assert_eq!(
        fixture.session.snapshot().unwrap().automatic_advance_count,
        0
    );

    let bridge = fixture.bridge.lock().unwrap().clone().unwrap();
    bridge.emit(24, AndroidPlayerEvent::AdvancedToNext);
    assert_eq!(
        fixture.session.snapshot().unwrap().automatic_advance_count,
        1
    );
    bridge.emit(
        24,
        AndroidPlayerEvent::Position {
            position_ms: 1_250,
            duration_ms: 180_000,
        },
    );
    assert_eq!(
        fixture.session.snapshot().unwrap().automatic_advance_count,
        1
    );
}

#[test]
fn core_queue_starts_the_next_track_when_media3_reports_a_plain_end() {
    let fixture = recording_session();
    fixture
        .session
        .play_tracks(
            vec![10, 11],
            vec![
                "content://provider/first.flac".to_owned(),
                "content://provider/second.flac".to_owned(),
            ],
            0,
        )
        .unwrap();
    fixture.calls.lock().unwrap().clear();
    let bridge = fixture.bridge.lock().unwrap().clone().unwrap();

    bridge.emit(23, AndroidPlayerEvent::TrackFinished);

    assert_eq!(fixture.session.snapshot().unwrap().current_index, Some(1));
    assert_eq!(
        fixture.calls.lock().unwrap().as_slice(),
        &[
            PortCall::PlayUri("content://provider/second.flac".to_owned()),
            PortCall::CurrentGeneration,
            PortCall::SetNext(None),
        ]
    );
}

#[test]
fn exported_session_seek_reaches_the_media3_port() {
    let fixture = recording_session();
    fixture.calls.lock().unwrap().clear();

    fixture.session.seek_to(48_000).unwrap();

    assert_eq!(
        fixture.calls.lock().unwrap().as_slice(),
        &[PortCall::SeekTo(48_000)]
    );
}

/// Two tracks, not one. A single-track queue wraps to index 0 under
/// `Repeat::All` just as it stays there under `Repeat::One`, so it cannot tell
/// the two modes apart — mapping `AndroidRepeatMode::One` to `Repeat::All`
/// passes such a fixture. With a second track the modes disagree about where
/// the playhead lands, which is the whole assertion.
#[test]
fn session_modes_are_readable_and_repeat_one_refeeds_after_media3_auto_advance() {
    let fixture = recording_session();
    fixture
        .session
        .play_tracks(
            vec![10, 11],
            vec![
                "content://provider/first.flac".to_owned(),
                "content://provider/second.flac".to_owned(),
            ],
            0,
        )
        .unwrap();

    fixture.session.set_shuffle(true).unwrap();
    fixture.session.set_repeat(AndroidRepeatMode::All).unwrap();
    let snapshot = fixture.session.snapshot().unwrap();
    assert!(snapshot.shuffled);
    assert_eq!(snapshot.repeat, AndroidRepeatMode::All);

    let bridge = fixture.bridge.lock().unwrap().clone().unwrap();
    bridge.emit(24, AndroidPlayerEvent::AdvancedToNext);
    assert_eq!(
        fixture.session.snapshot().unwrap().current_index,
        Some(1),
        "Repeat::All must leave the track it just finished",
    );

    fixture
        .session
        .play_tracks(
            vec![10, 11],
            vec![
                "content://provider/first.flac".to_owned(),
                "content://provider/second.flac".to_owned(),
            ],
            0,
        )
        .unwrap();
    fixture.session.set_repeat(AndroidRepeatMode::One).unwrap();
    assert_eq!(
        fixture.session.snapshot().unwrap().repeat,
        AndroidRepeatMode::One
    );
    fixture.calls.lock().unwrap().clear();

    bridge.emit(24, AndroidPlayerEvent::AdvancedToNext);

    assert_eq!(
        fixture.session.snapshot().unwrap().current_index,
        Some(0),
        "Repeat::One must stay on the track Media3 just advanced past",
    );
    assert_eq!(
        fixture.calls.lock().unwrap().as_slice(),
        &[PortCall::SetNext(Some(
            "content://provider/first.flac".to_owned()
        ))],
        "Repeat::One must re-feed the real AdvancedToNext path Media3 emits",
    );
}

#[test]
fn play_count_uses_the_tracks_high_water_position_and_records_only_once() {
    let directory = tempfile::tempdir().unwrap();
    let music = directory.path().join("music");
    std::fs::create_dir(&music).unwrap();
    let track_path = music.join("sine.flac");
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../android/app/src/main/assets/sine.flac"),
        &track_path,
    )
    .unwrap();
    let db_path = directory.path().join("reprise.db");
    let db = reprise_core::db::Db::open_migrated(Some(&db_path)).unwrap();
    reprise_core::library::scanner::scan_folder(&db, &music).unwrap();
    let track = reprise_core::queries::query_library_text_search(
        &db,
        "",
        reprise_core::queries::WindowRange {
            offset: 0,
            limit: 1,
        },
    )
    .unwrap()
    .rows
    .remove(0);
    drop(db);

    let calls = Arc::new(Mutex::new(Vec::new()));
    let bridge = Arc::new(Mutex::new(None));
    let report_changes = Arc::new(AtomicUsize::new(0));
    let session = AndroidPlaybackSession::new(
        directory.path().to_str().unwrap(),
        Box::new(RecordingPort {
            calls,
            bridge: Arc::clone(&bridge),
        }),
        Box::new(RecordingListener {
            snapshots: Arc::new(Mutex::new(Vec::new())),
            report_changes: Arc::clone(&report_changes),
        }),
    )
    .unwrap();
    session
        .play_tracks(vec![track.id], vec![track.path], 0)
        .unwrap();
    let events = bridge.lock().unwrap().clone().unwrap();

    events.emit(
        23,
        AndroidPlayerEvent::Position {
            position_ms: 600,
            // Media3 can know the position before it has resolved duration;
            // this tick cannot count yet, but its high-water must survive.
            duration_ms: 0,
        },
    );
    events.emit(
        23,
        AndroidPlayerEvent::Position {
            position_ms: 100,
            duration_ms: 1_000,
        },
    );
    // The write is queued to the session's writer thread rather than done on
    // the thread Media3 calls in on. Dropping the session is what the service's
    // `onDestroy` does, and it must drain what is still queued — so this is
    // both how the assertion becomes deterministic and the proof that a play
    // counted during teardown survives it.
    drop(session);
    assert_eq!(report_changes.load(Ordering::Relaxed), 1);

    let verify = reprise_core::db::Db::open_ready(&db_path).unwrap();
    let updated = reprise_core::queries::query_library_text_search(
        &verify,
        "",
        reprise_core::queries::WindowRange {
            offset: 0,
            limit: 1,
        },
    )
    .unwrap()
    .rows
    .remove(0);
    assert_eq!(updated.play_count, 1);
    let library = crate::MusicLibrary::open(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
    )
    .unwrap();
    let report = reprise_core::device_sync::listen_report::ListenReport::decode(
        &library.prepare_listen_report(None).unwrap(),
    )
    .unwrap();
    assert_eq!(report.listens.len(), 1);
    assert_eq!(report.listens[0].sequence, 1);
    assert_eq!(report.listens[0].device_path, "sine.flac");
    assert_eq!(report.listens[0].ms_played, 600);
    assert!(report.listens[0].played_at > 0);
    assert!(report.ratings.is_empty());
}

#[test]
fn viewing_and_applying_playback_settings_preserves_the_authored_curve_byte_for_byte() {
    let directory = tempfile::tempdir().unwrap();
    let library = crate::MusicLibrary::open(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
    )
    .unwrap();
    let curve = reprise_core::equalizer::EqualizerCurve::new(vec![
        reprise_core::equalizer::EqualizerPoint {
            frequency_hz: 80.0,
            gain_db: -4.5,
        },
        reprise_core::equalizer::EqualizerPoint {
            frequency_hz: 12_000.0,
            gain_db: 7.25,
        },
    ])
    .unwrap();
    let stored_before = {
        let state = library.lock().unwrap();
        reprise_core::library::settings::set_equalizer_curve(&state.db, &curve).unwrap();
        reprise_core::library::settings::set_equalizer_enabled(&state.db, true).unwrap();
        reprise_core::library::settings::get_setting(
            &state.db,
            reprise_core::library::settings::EQUALIZER_CURVE_KEY,
        )
        .unwrap()
    };

    let viewed = library.playback_settings().unwrap();

    assert_eq!(viewed.equalizer_curve.len(), 2);
    assert_eq!(viewed.equalizer_curve[0].frequency_hz, 80.0);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let session = AndroidPlaybackSession::new(
        directory.path().to_str().unwrap(),
        Box::new(RecordingPort {
            calls: Arc::clone(&calls),
            bridge: Arc::new(Mutex::new(None)),
        }),
        Box::new(RecordingListener {
            snapshots: Arc::new(Mutex::new(Vec::new())),
            report_changes: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .unwrap();
    assert!(calls.lock().unwrap().contains(&PortCall::SetEqualizer(
        true,
        viewed.equalizer_curve.clone(),
    )));
    drop(session);
    let stored_after = {
        let state = library.lock().unwrap();
        reprise_core::library::settings::get_setting(
            &state.db,
            reprise_core::library::settings::EQUALIZER_CURVE_KEY,
        )
        .unwrap()
    };
    assert_eq!(
        stored_after, stored_before,
        "viewing and applying must never write a projection"
    );
}

#[test]
fn phone_curve_replacement_validates_its_numeric_payload_and_changes_only_that_key() {
    let directory = tempfile::tempdir().unwrap();
    let library = crate::MusicLibrary::open(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
    )
    .unwrap();
    {
        let state = library.lock().unwrap();
        reprise_core::library::settings::set_setting(&state.db, "ui.theme", "desktop-only-theme")
            .unwrap();
    }

    library
        .replace_equalizer_curve(vec![
            AndroidEqualizerPoint {
                frequency_hz: 125.0,
                gain_db: -3.0,
            },
            AndroidEqualizerPoint {
                frequency_hz: 1_000.0,
                gain_db: 4.5,
            },
        ])
        .unwrap();
    let saved = library.playback_settings().unwrap();
    assert_eq!(saved.equalizer_curve.len(), 2);
    assert_eq!(saved.equalizer_curve[1].gain_db, 4.5);
    assert!(library
        .replace_equalizer_curve(vec![
            AndroidEqualizerPoint {
                frequency_hz: 1_000.0,
                gain_db: f64::NAN,
            },
            AndroidEqualizerPoint {
                frequency_hz: 125.0,
                gain_db: 0.0,
            },
        ])
        .is_err());

    let state = library.lock().unwrap();
    assert_eq!(
        reprise_core::library::settings::get_setting(&state.db, "ui.theme")
            .unwrap()
            .as_deref(),
        Some("desktop-only-theme"),
    );
    assert_eq!(
        reprise_core::library::settings::get_equalizer_curve(&state.db)
            .points()
            .len(),
        2,
        "a rejected replacement must leave the authored curve intact",
    );
}

#[test]
fn saved_track_transition_drives_android_at_startup_and_after_reload() {
    let directory = tempfile::tempdir().unwrap();
    let database_path = directory.path().join("reprise.db");
    let database = reprise_core::db::Db::open_migrated(Some(&database_path)).unwrap();
    reprise_core::library::settings::set_gapless_enabled(&database, false).unwrap();
    drop(database);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let session = AndroidPlaybackSession::new(
        directory.path().to_str().unwrap(),
        Box::new(RecordingPort {
            calls: Arc::clone(&calls),
            bridge: Arc::new(Mutex::new(None)),
        }),
        Box::new(RecordingListener {
            snapshots: Arc::new(Mutex::new(Vec::new())),
            report_changes: Arc::new(AtomicUsize::new(0)),
        }),
    )
    .unwrap();
    assert!(calls
        .lock()
        .unwrap()
        .contains(&PortCall::SetTransition(AndroidTransitionMode::Off)));

    let library = crate::MusicLibrary::open(
        directory.path().to_str().unwrap(),
        directory.path().join("cache").to_str().unwrap(),
    )
    .unwrap();
    library.set_gapless_enabled(true).unwrap();
    session.reload_playback_settings().unwrap();

    assert_eq!(
        calls.lock().unwrap().last(),
        Some(&PortCall::SetTransition(AndroidTransitionMode::Gapless)),
    );
}

#[test]
fn playback_event_bridge_delivers_ordered_core_events_with_production_generations() {
    let received = Arc::new(Mutex::new(Vec::<StreamEvent>::new()));
    let recorded = Arc::clone(&received);
    let bridge = PlaybackEventBridge::new(Box::new(move |event| {
        recorded.lock().unwrap().push(event);
    }));

    bridge.emit(
        7,
        AndroidPlayerEvent::StateChanged {
            state: AndroidPlaybackState::Playing,
        },
    );
    bridge.emit(
        7,
        AndroidPlayerEvent::Position {
            position_ms: 1_250,
            duration_ms: 180_000,
        },
    );
    bridge.emit(8, AndroidPlayerEvent::AdvancedToNext);
    bridge.emit(8, AndroidPlayerEvent::TrackFinished);
    bridge.emit(
        8,
        AndroidPlayerEvent::Error {
            message: "decoder failed".to_owned(),
        },
    );

    let events = received.lock().unwrap();
    assert_eq!(events.len(), 5);
    assert_eq!(events[0].generation, StreamGeneration::from(7));
    assert!(matches!(
        events[0].event,
        PlayerEvent::StateChanged(PlaybackState::Playing)
    ));
    assert_eq!(events[1].generation, StreamGeneration::from(7));
    assert!(matches!(
        events[1].event,
        PlayerEvent::Position {
            position_ms: 1_250,
            duration_ms: 180_000
        }
    ));
    assert_eq!(events[2].generation, StreamGeneration::from(8));
    assert!(matches!(events[2].event, PlayerEvent::AdvancedToNext));
    assert_eq!(events[3].generation, StreamGeneration::from(8));
    assert!(matches!(events[3].event, PlayerEvent::TrackFinished));
    assert_eq!(events[4].generation, StreamGeneration::from(8));
    assert!(matches!(
        &events[4].event,
        PlayerEvent::Error(message) if message == "decoder failed"
    ));
}

#[test]
fn media3_buffering_survives_the_core_event_bridge_in_the_android_snapshot() {
    let fixture = recording_session();
    fixture
        .session
        .play_tracks(vec![7], vec!["content://provider/song.flac".to_owned()], 0)
        .unwrap();
    let bridge = fixture.bridge.lock().unwrap().clone().unwrap();

    bridge.emit(
        23,
        AndroidPlayerEvent::StateChanged {
            state: AndroidPlaybackState::Buffering,
        },
    );

    assert_eq!(
        fixture.session.snapshot().unwrap().state,
        AndroidPlaybackState::Buffering,
    );
}
