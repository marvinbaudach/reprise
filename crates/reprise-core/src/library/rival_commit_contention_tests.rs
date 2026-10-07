//! Settings-style facades that wrap a read-then-write helper in their own
//! transaction must survive a rival commit landing between the read and the
//! write (#1188). A deferred transaction pins a read snapshot first and then
//! fails the write-lock upgrade with `SQLITE_BUSY_SNAPSHOT`, which
//! `busy_timeout` never retries. The shared fixture places the rival commit
//! deterministically; see `rival_commit_test_support`.

use std::sync::atomic::Ordering;
use std::time::Duration;

use super::rival_commit_test_support::{arm, arm_on_first_write};
use super::settings::{get_setting_in, ReplayGainMode};
use crate::db::Db;
use crate::playback::AudioEffects;

fn contended_db() -> (tempfile::TempDir, Db) {
    let directory = tempfile::tempdir().unwrap();
    let db = Db::open_migrated(Some(&directory.path().join("reprise.db"))).unwrap();
    (directory, db)
}

fn changed_effects() -> AudioEffects {
    AudioEffects {
        equalizer_enabled: true,
        equalizer_bands: [4.0; 10],
        replay_gain: ReplayGainMode::Album,
    }
}

#[test]
fn storing_audio_effects_survives_a_rival_commit_between_its_read_and_its_write() {
    let (directory, db) = contended_db();
    let interleaved = arm(db.conn(), &directory.path().join("reprise.db"), "settings");

    super::audio_effect_settings::store(&db, &changed_effects()).unwrap();

    assert!(
        interleaved.load(Ordering::SeqCst),
        "the rival must have committed mid-transaction"
    );
    assert_eq!(super::audio_effect_settings::load(&db), changed_effects());
}

#[test]
fn storing_unchanged_audio_effects_never_waits_for_the_write_lock() {
    let (directory, db) = contended_db();
    db.conn().pragma_update(None, "busy_timeout", 0).unwrap();
    super::audio_effect_settings::store(&db, &changed_effects()).unwrap();
    let holder = rusqlite::Connection::open(directory.path().join("reprise.db")).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();

    super::audio_effect_settings::store(&db, &changed_effects()).unwrap();

    assert!(
        super::audio_effect_settings::store(&db, &AudioEffects::default()).is_err(),
        "changed effects do need the write lock the holder keeps",
    );
}

#[test]
fn recording_doctor_scan_rates_survives_a_rival_commit_between_its_read_and_its_write() {
    let (directory, db) = contended_db();
    let interleaved = arm(db.conn(), &directory.path().join("reprise.db"), "settings");

    super::library_doctor::record_scan_rates(
        &db,
        120,
        Duration::from_secs(60),
        Some(Duration::from_secs(120)),
    )
    .unwrap();

    assert!(
        interleaved.load(Ordering::SeqCst),
        "the rival must have committed mid-transaction"
    );
    let rates = super::library_doctor::scan_rates(&db).unwrap();
    assert_eq!(rates.local_tracks_per_minute, Some(120.0));
    assert_eq!(rates.remote_tracks_per_minute, Some(60.0));
}

#[test]
fn accepting_remote_suggestions_survives_a_rival_commit_between_its_read_and_its_write() {
    let (directory, db) = contended_db();
    let interleaved = arm(db.conn(), &directory.path().join("reprise.db"), "settings");

    super::library_doctor::accept_remote_suggestions(&db).unwrap();

    assert!(
        interleaved.load(Ordering::SeqCst),
        "the rival must have committed mid-transaction"
    );
    let preference = super::library_doctor::remote_suggestion_preference(&db).unwrap();
    assert!(preference.enabled && !preference.consent_required);
}

/// `mark_device_playlists_synced` opens with an UPDATE, so its first statement
/// already takes the write lock: the audit leaves it deferred, and this keeps
/// that reading honest.
#[test]
fn marking_device_playlists_synced_opens_with_a_write_and_survives_a_rival_commit() {
    use crate::device_sync::settings::{
        load_device_playlists, mark_device_playlists_synced, upsert_device_playlist,
        DevicePlaylistRecord, SelectionSource,
    };

    let (directory, db) = contended_db();
    upsert_device_playlist(
        &db,
        &DevicePlaylistRecord {
            device_serial: "phone".into(),
            source: SelectionSource::Playlist(42),
            source_name: "Road Trip".into(),
            device_path: "Reprise/Playlists/Road Trip.m3u8".into(),
            last_synced_at: None,
        },
    )
    .unwrap();
    let interleaved = arm_on_first_write(
        db.conn(),
        &directory.path().join("reprise.db"),
        "device_playlists",
    );

    mark_device_playlists_synced(
        &db,
        "phone",
        &[SelectionSource::Playlist(42)],
        1_753_612_496,
    )
    .unwrap();

    assert!(
        interleaved.load(Ordering::SeqCst),
        "the rival must have committed mid-transaction"
    );
    assert_eq!(
        load_device_playlists(&db, "phone").unwrap()[0].last_synced_at,
        Some(1_753_612_496)
    );
    assert_eq!(
        get_setting_in(db.conn(), "rival").unwrap().as_deref(),
        Some("1")
    );
}
