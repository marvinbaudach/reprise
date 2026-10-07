//! Facades whose transaction starts with a read must survive a rival commit
//! landing between that read and their first write (#1188). A deferred
//! transaction pins a read snapshot first and then fails the write-lock
//! upgrade with `SQLITE_BUSY_SNAPSHOT` (extended code 517), which
//! `busy_timeout` never retries. The shared fixture places the rival commit
//! deterministically; see `library::rival_commit_test_support`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::db::{Db, SpectrogramStoreOutcome};
use crate::library::rival_commit_test_support::{arm, arm_on_first_write};
use crate::library::{playlist_membership, playlists, rhythmbox_import};
use crate::queries::{tombstone_still_missing, MissingGroupKind};
use crate::radio::station::{self, NewStation};
use crate::spectrogram::{TrackSourceFingerprint, TrackSpectrogram};
use crate::waveform::TrackRenderData;

const DB_FILE: &str = "reprise.db";

fn contended_db() -> (tempfile::TempDir, Db) {
    let directory = tempfile::tempdir().unwrap();
    let db = Db::open_migrated(Some(&directory.path().join(DB_FILE))).unwrap();
    (directory, db)
}

fn arm_first_write(directory: &tempfile::TempDir, db: &Db, table: &'static str) -> Arc<AtomicBool> {
    arm_on_first_write(db.conn(), &directory.path().join(DB_FILE), table)
}

fn assert_interleaved(flag: &AtomicBool) {
    assert!(
        flag.load(Ordering::SeqCst),
        "the rival must have committed mid-transaction"
    );
}

fn insert_track(db: &Db, id: i64) {
    db.conn()
        .execute(
            "INSERT INTO tracks \
             (id, path, title, added_at, file_mtime, file_size, device, inode) \
             VALUES (?1, ?2, 'Track', 0, 11, 22, 33, 44)",
            rusqlite::params![id, format!("/music/{id}.flac")],
        )
        .unwrap();
}

fn fingerprint() -> TrackSourceFingerprint {
    TrackSourceFingerprint {
        mtime_seconds: 11,
        size_bytes: 22,
        device: Some(33),
        inode: Some(44),
    }
}

#[test]
fn storing_a_spectrogram_survives_a_rival_commit_between_its_read_and_its_write() {
    let (directory, db) = contended_db();
    insert_track(&db, 1);
    let flag = arm(
        db.conn(),
        &directory.path().join(DB_FILE),
        "track_spectrograms",
    );

    let outcome =
        crate::db::set_track_spectrogram(&db, 1, fingerprint(), &TrackSpectrogram::empty())
            .unwrap();

    assert_interleaved(&flag);
    assert_eq!(outcome, SpectrogramStoreOutcome::Stored);
}

#[test]
fn storing_render_data_survives_a_rival_commit_between_its_read_and_its_write() {
    let (directory, db) = contended_db();
    insert_track(&db, 1);
    let flag = arm_first_write(&directory, &db, "tracks");

    let outcome =
        crate::db::set_track_render_data(&db, 1, fingerprint(), &TrackRenderData::empty()).unwrap();

    assert_interleaved(&flag);
    assert_eq!(outcome, SpectrogramStoreOutcome::Stored);
}

#[test]
fn deleting_a_playlist_survives_a_rival_commit_between_its_read_and_its_write() {
    let (directory, db) = contended_db();
    let id = playlists::create(&db, "Doomed").unwrap();
    let flag = arm_first_write(&directory, &db, "playlists");

    let deleted = playlists::delete(&db, id, "Doomed").unwrap();

    assert_interleaved(&flag);
    assert!(deleted);
}

#[test]
fn adding_unique_tracks_survives_a_rival_commit_between_its_read_and_its_write() {
    let (directory, db) = contended_db();
    insert_track(&db, 1);
    insert_track(&db, 2);
    let id = playlists::create(&db, "Contested").unwrap();
    let flag = arm(
        db.conn(),
        &directory.path().join(DB_FILE),
        "playlist_tracks",
    );

    let added = playlist_membership::add_unique_tracks(&db, id, &[1, 2]).unwrap();

    assert_interleaved(&flag);
    assert_eq!(added, 2);
}

#[test]
fn adding_a_radio_station_survives_a_rival_commit_between_its_read_and_its_write() {
    let (directory, db) = contended_db();
    let flag = arm(db.conn(), &directory.path().join(DB_FILE), "radio_stations");
    let new_station = NewStation {
        uuid: Some("station-1".into()),
        name: "Contested FM".into(),
        stream_url: "https://radio.example/stream".into(),
        homepage: None,
        favicon_url: None,
        genre: None,
        codec: None,
        bitrate_kbps: None,
        country_code: None,
        votes: None,
    };

    let id = station::add_or_restore(&db, &new_station, 1_000).unwrap();

    assert_interleaved(&flag);
    assert!(id > 0);
}

#[test]
fn tombstoning_missing_tracks_survives_a_rival_commit_between_its_read_and_its_write() {
    let (directory, db) = contended_db();
    db.conn()
        .execute(
            "INSERT INTO tracks (id, path, title, added_at, missing_since, missing_reason) \
             VALUES (1, '/music/gone.flac', 'Gone', 0, 10, 'deleted')",
            [],
        )
        .unwrap();
    let flag = arm_first_write(&directory, &db, "tracks");

    let tombstoned = tombstone_still_missing(&db, &MissingGroupKind::Deleted, &[1], 1_000).unwrap();

    assert_interleaved(&flag);
    assert_eq!(tombstoned, vec![1]);
}

#[test]
fn merging_rhythmbox_stats_survives_a_rival_commit_between_its_read_and_its_write() {
    let (directory, db) = contended_db();
    insert_track(&db, 1);
    let flag = arm_first_write(&directory, &db, "tracks");
    let stats = rhythmbox_import::RhythmboxTrackStats {
        path: "/music/1.flac".into(),
        rating: Some(4),
        play_count: None,
        added_at: None,
        last_played_at: None,
    };
    let choices = rhythmbox_import::RhythmboxImportChoices {
        ratings: true,
        play_counts_and_last_played: false,
        added_at: false,
    };

    let (summary, _) = rhythmbox_import::merge_stats(&db, &[stats], choices, None).unwrap();

    assert_interleaved(&flag);
    assert_eq!(summary.parsed, 1);
}

fn insert_hidden_release(db: &Db, mbid: &str) {
    db.conn()
        .execute(
            "INSERT INTO new_releases (
               release_group_mbid, artist_name, artist_mbid, title, release_type,
               first_release_date, fetched_at, hidden, hidden_at
             ) VALUES (?1, 'Artist', 'artist-id', ?1, 'Album', '2026-08-01', 1, 1, 5)",
            [mbid],
        )
        .unwrap();
}

fn is_hidden(db: &Db, mbid: &str) -> bool {
    db.conn()
        .query_row(
            "SELECT hidden FROM new_releases WHERE release_group_mbid = ?1",
            [mbid],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn showing_a_hidden_release_again_survives_a_rival_commit_between_its_read_and_its_write() {
    let (directory, db) = contended_db();
    insert_hidden_release(&db, "one");
    let flag = arm_first_write(&directory, &db, "new_releases");

    crate::artist_news::set_release_hidden(&db, "one", false).unwrap();

    assert_interleaved(&flag);
    assert!(!is_hidden(&db, "one"));
}

#[test]
fn showing_hidden_releases_again_survives_a_rival_commit_between_its_read_and_its_write() {
    let (directory, db) = contended_db();
    insert_hidden_release(&db, "one");
    let flag = arm_first_write(&directory, &db, "new_releases");

    crate::artist_news::set_releases_hidden(&db, &["one".to_string()], false).unwrap();

    assert_interleaved(&flag);
    assert!(!is_hidden(&db, "one"));
}

#[test]
fn restoring_a_release_survives_a_rival_commit_between_its_read_and_its_write() {
    let (directory, db) = contended_db();
    insert_hidden_release(&db, "one");
    let flag = arm_first_write(&directory, &db, "new_releases");

    crate::artist_news_history::restore_release(&db, "one").unwrap();

    assert_interleaved(&flag);
    assert!(!is_hidden(&db, "one"));
}

#[test]
fn reconciling_deleted_release_memory_survives_a_rival_commit_between_its_read_and_its_write() {
    let (directory, db) = contended_db();
    db.conn()
        .execute_batch(
            "INSERT INTO new_releases (
               release_group_mbid, artist_name, artist_mbid, title, release_type,
               first_release_date, fetched_at, first_seen, hidden, hidden_at,
               hidden_by_deleted_memory
             ) VALUES ('album', 'Release Artist', 'artist-id', 'Ghost', 'Album',
                       '2026-08-01', 1, 1, 1, 10, 1);
             INSERT INTO tracks (id, path, title, artist, album_artist, album, added_at)
             VALUES (1, '/music/1.flac', 'Ghost', 'Track Artist', 'Release Artist', 'Ghost', 0);
             INSERT INTO deleted_releases (artist_key, title_key, scope, deleted_at)
             VALUES ('release artist', 'ghost', 'album', 10);",
        )
        .unwrap();
    let flag = arm_first_write(&directory, &db, "deleted_releases");

    crate::artist_news_pipeline::reconcile_deleted_release_memory(db.conn()).unwrap();

    assert_interleaved(&flag);
}

/// Adding only ids that are already members changes nothing, so it must not
/// wait for the write lock another connection holds.
#[test]
fn adding_only_existing_members_never_waits_for_the_write_lock() {
    let (directory, db) = contended_db();
    insert_track(&db, 1);
    let id = playlists::create(&db, "Settled").unwrap();
    playlist_membership::add_unique_tracks(&db, id, &[1]).unwrap();
    db.conn().pragma_update(None, "busy_timeout", 0).unwrap();
    let holder = rusqlite::Connection::open(directory.path().join(DB_FILE)).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();

    let added = playlist_membership::add_unique_tracks(&db, id, &[1, 1]).unwrap();

    assert_eq!(added, 0);
}
