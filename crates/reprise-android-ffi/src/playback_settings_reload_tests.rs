use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};

use super::test_support::{library_in, PortCall, RecordingListener, RecordingPort};
use crate::playback::AndroidTransitionMode;
use crate::{AndroidEqualizerPoint, AndroidPlaybackSession};

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
        let writer = library.writer().unwrap();
        reprise_core::library::settings::set_equalizer_curve(&writer, &curve).unwrap();
        reprise_core::library::settings::set_equalizer_enabled(&writer, true).unwrap();
        reprise_core::library::settings::get_setting(
            &writer,
            reprise_core::library::settings::EQUALIZER_CURVE_KEY,
        )
        .unwrap()
    };

    let viewed = library.playback_settings().unwrap();

    assert_eq!(viewed.equalizer_curve.len(), 2);
    assert_eq!(viewed.equalizer_curve[0].frequency_hz, 80.0);
    let calls = Arc::new(Mutex::new(Vec::new()));
    let session = AndroidPlaybackSession::new(
        library_in(directory.path()),
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
        let writer = library.writer().unwrap();
        reprise_core::library::settings::get_setting(
            &writer,
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
        let writer = library.writer().unwrap();
        reprise_core::library::settings::set_setting(&writer, "ui.theme", "desktop-only-theme")
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

    let writer = library.writer().unwrap();
    assert_eq!(
        reprise_core::library::settings::get_setting(&writer, "ui.theme")
            .unwrap()
            .as_deref(),
        Some("desktop-only-theme"),
    );
    assert_eq!(
        reprise_core::library::settings::get_equalizer_curve(&writer)
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
        library_in(directory.path()),
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
