use crate::sound_features::{derive_sound_features, SoundFeatures};
use crate::spectrogram::{TrackSpectrogram, SPECTROGRAM_BAND_COUNT};
use crate::{
    db::{get_track_sound_features, set_track_sound_features, Db},
    spectrogram::SPECTROGRAM_FORMAT_VERSION,
};

fn frame(active_band: usize, level: u8) -> Vec<u8> {
    let mut cells = vec![0; SPECTROGRAM_BAND_COUNT];
    cells[active_band] = level;
    cells
}

fn spectrogram(frames: impl IntoIterator<Item = Vec<u8>>) -> TrackSpectrogram {
    TrackSpectrogram::from_cells(frames.into_iter().flatten().collect()).unwrap()
}

fn assert_near(actual: f32, expected: f32, tolerance: f32) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {actual} to be within {tolerance} of {expected}"
    );
}

#[test]
fn sound_features_empty_spectrogram_has_a_finite_neutral_profile() {
    assert_eq!(
        derive_sound_features(&TrackSpectrogram::empty()),
        SoundFeatures {
            band_mean: [0.0; SPECTROGRAM_BAND_COUNT],
            centroid_mean: 0.0,
            centroid_var: 0.0,
            frame_crest_db: 0.0,
            tempo: None,
        }
    );
}

#[test]
fn sound_features_band_means_are_l2_normalized_and_centroids_use_band_indices() {
    let features = derive_sound_features(&spectrogram([
        frame(3, 255),
        frame(3, 255),
        frame(9, 255),
        frame(9, 255),
    ]));

    let norm = features
        .band_mean
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    assert_near(norm, 1.0, 1.0e-5);
    assert_near(features.band_mean[3], 2.0_f32.sqrt().recip(), 1.0e-5);
    assert_near(features.band_mean[9], 2.0_f32.sqrt().recip(), 1.0e-5);
    assert_near(features.centroid_mean, 6.0, 1.0e-5);
    assert_near(features.centroid_var, 9.0, 1.0e-5);
}

#[test]
fn sound_features_frame_crest_compares_the_loudest_frame_with_the_mean() {
    let features = derive_sound_features(&spectrogram([
        frame(4, 255),
        frame(4, 0),
        frame(4, 0),
        frame(4, 0),
    ]));

    assert_near(features.frame_crest_db, 10.0 * 4.0_f32.log10(), 0.05);
}

#[test]
fn sound_features_tempo_uses_periodic_bass_onsets_and_rejects_flat_energy() {
    let mut pulsed = Vec::new();
    for index in 0..120 {
        pulsed.push(frame(2, if index % 10 == 0 { 255 } else { 0 }));
    }
    let pulsed = derive_sound_features(&spectrogram(pulsed));
    assert_near(
        pulsed.tempo.expect("periodic onsets need a tempo"),
        120.0,
        0.1,
    );

    let flat = derive_sound_features(&spectrogram((0..120).map(|_| frame(2, 180))));
    assert_eq!(flat.tempo, None);
}

fn stored_features() -> SoundFeatures {
    SoundFeatures {
        band_mean: std::array::from_fn(|index| index as f32 / 100.0),
        centroid_mean: 4.25,
        centroid_var: 2.5,
        frame_crest_db: 7.75,
        tempo: Some(123.5),
    }
}

fn insert_track(db: &Db) {
    db.conn()
        .execute(
            "INSERT INTO tracks \
             (id, path, title, added_at, file_mtime, file_size, device, inode) \
             VALUES (1, '/sound.flac', 'Sound', 0, 11, 22, 33, 44)",
            [],
        )
        .unwrap();
}

#[test]
fn sound_features_store_round_trips_and_rejects_an_old_spectrogram_format() {
    let db = Db::open_in_memory().unwrap();
    insert_track(&db);
    set_track_sound_features(&db, 1, &stored_features()).unwrap();
    assert_eq!(
        get_track_sound_features(&db, 1).unwrap(),
        Some(stored_features())
    );

    db.conn()
        .execute(
            "UPDATE track_sound_features SET format_version = ?1 WHERE track_id = 1",
            [SPECTROGRAM_FORMAT_VERSION + 1],
        )
        .unwrap();
    assert_eq!(get_track_sound_features(&db, 1).unwrap(), None);
}

#[test]
fn sound_features_follow_track_cascade_and_source_invalidation() {
    let db = Db::open_in_memory().unwrap();
    insert_track(&db);
    set_track_sound_features(&db, 1, &stored_features()).unwrap();
    db.conn()
        .execute("UPDATE tracks SET file_size = 23 WHERE id = 1", [])
        .unwrap();
    assert_eq!(get_track_sound_features(&db, 1).unwrap(), None);

    set_track_sound_features(&db, 1, &stored_features()).unwrap();
    db.conn()
        .execute("DELETE FROM tracks WHERE id = 1", [])
        .unwrap();
    let rows: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM track_sound_features", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(rows, 0);
}

#[test]
fn sound_features_v56_repairs_a_database_already_stamped_by_the_other_v56_step() {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate_connection(&conn).unwrap();
    conn.execute("DROP TABLE track_sound_features", []).unwrap();
    assert_eq!(
        conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        56
    );

    crate::db_sound_features::migrate_v56(&conn).unwrap();
    crate::db_sound_features::migrate_v56(&conn).unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name = 'track_sound_features'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}
