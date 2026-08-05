use crate::sound_features::{
    derive_sound_features, sound_features_stamp, SoundFeatures, SOUND_FEATURES_FORMAT_VERSION,
    SOUND_FEATURE_LAYOUT_VERSION,
};
use crate::sound_rhythm::{derive_rhythm_features, RhythmFeatures};
use crate::spectrogram::{TrackSpectrogram, SPECTROGRAM_BAND_COUNT};
use crate::{
    db::{
        get_track_sound_features, set_track_render_data, set_track_sound_features, Db,
        SpectrogramStoreOutcome,
    },
    spectrogram::{TrackSourceFingerprint, SPECTROGRAM_FORMAT_VERSION},
    waveform::TrackRenderData,
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
            rhythm: RhythmFeatures::still(),
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
        rhythm: RhythmFeatures {
            band_flux: std::array::from_fn(|index| index as f32 / 200.0),
            onset_rate: 3.25,
            flux_mean: 12.5,
            flux_variation: 0.75,
            pulse_strength: 0.5,
        },
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
fn sim_1_sound_profile_cache_rejects_a_stale_spectrogram_format() {
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
            [SOUND_FEATURES_FORMAT_VERSION + 1],
        )
        .unwrap();
    assert_eq!(get_track_sound_features(&db, 1).unwrap(), None);
}

#[test]
fn sim_1_sound_profile_cache_follows_track_and_source_invalidation() {
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

/// The two v56 steps no longer share a number: the accent drop keeps 56 and
/// the profile cache owns 57, so `user_version` alone says whether
/// `track_sound_features` is there.
///
/// Both starting points have to land on the same schema — a plain v56 database
/// that never saw this branch, and one an earlier build of this branch already
/// stamped 56 *with* the table and the extended trigger in it.
#[test]
fn sound_features_v57_upgrades_a_plain_v56_and_one_this_branch_already_stamped() {
    for branch_already_stamped in [false, true] {
        let conn = crate::db::open(None).unwrap();
        crate::db::migrate_connection(&conn).unwrap();
        if !branch_already_stamped {
            conn.execute("DROP TRIGGER invalidate_track_render_data", [])
                .unwrap();
            conn.execute("DROP TABLE track_sound_features", []).unwrap();
        }
        conn.pragma_update(None, "user_version", 56).unwrap();

        crate::db_sound_features::migrate_v57(&conn).unwrap();
        crate::db_sound_features::migrate_v57(&conn).unwrap(); // idempotent re-run must not fail

        assert_eq!(
            conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            57
        );
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name = 'track_sound_features'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        let trigger: String = conn
            .query_row(
                "SELECT sql FROM sqlite_schema \
                 WHERE type = 'trigger' AND name = 'invalidate_track_render_data'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(trigger.contains("track_sound_features"));
    }
}

/// A library still on the shared 56 is not ready for a reader that expects the
/// profile cache, and now says so instead of failing later on `no such table`.
#[test]
fn sim_1_a_database_without_the_profile_cache_is_not_ready() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("library.db");
    let conn = crate::db::open(Some(&path)).unwrap();
    crate::db::migrate_connection(&conn).unwrap();
    conn.pragma_update(None, "user_version", 56).unwrap();
    drop(conn);

    assert!(matches!(
        Db::open_ready(&path),
        Err(crate::db::DbError::SchemaNotReady { found, supported })
            if found == 56 && supported == crate::db::SUPPORTED_SCHEMA_VERSION
    ));
}

/// FNV-1a over the stored bytes — an order-sensitive canary on the blob layout.
fn blob_fingerprint(blob: &[u8]) -> u64 {
    blob.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[test]
fn sim_1_sound_profile_stamp_covers_the_spectrogram_format_and_the_blob_layout() {
    assert_eq!(
        SOUND_FEATURES_FORMAT_VERSION,
        sound_features_stamp(SPECTROGRAM_FORMAT_VERSION, SOUND_FEATURE_LAYOUT_VERSION)
    );
    assert_ne!(
        sound_features_stamp(SPECTROGRAM_FORMAT_VERSION + 1, SOUND_FEATURE_LAYOUT_VERSION),
        SOUND_FEATURES_FORMAT_VERSION,
        "a spectrogram format bump must invalidate the derived profiles"
    );
    assert_ne!(
        sound_features_stamp(SPECTROGRAM_FORMAT_VERSION, SOUND_FEATURE_LAYOUT_VERSION + 1),
        SOUND_FEATURES_FORMAT_VERSION,
        "a blob layout bump must invalidate the derived profiles"
    );

    // The layout is pinned to its version: changing the stored bytes moves this
    // fingerprint, and bumping SOUND_FEATURE_LAYOUT_VERSION in the same change
    // is what keeps rows written by the old layout out of the SQL filter.
    let blob = stored_features().to_blob();
    assert_eq!(blob.len(), 225);
    assert_eq!(blob_fingerprint(&blob), 0x3550_ade2_d381_bee9);
    assert_eq!(SOUND_FEATURE_LAYOUT_VERSION, 2);
}

#[test]
fn sound_profiles_skip_one_unreadable_row_and_keep_the_rest() {
    let db = Db::open_in_memory().unwrap();
    for id in [1, 2] {
        db.conn()
            .execute(
                "INSERT INTO tracks (id, path, title, added_at) VALUES (?1, ?2, ?3, 0)",
                rusqlite::params![id, format!("/{id}.flac"), format!("Sound {id}")],
            )
            .unwrap();
        set_track_sound_features(&db, id, &stored_features()).unwrap();
    }
    db.conn()
        .execute(
            "UPDATE track_sound_features SET data = ?1 WHERE track_id = 1",
            [vec![0_u8; 7]],
        )
        .unwrap();

    let rows = crate::db_sound_features::all_track_sound_features(&db).unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].track_id, 2);
    assert_eq!(rows[0].features, stored_features());
}

#[test]
fn sim_1_sound_profile_is_stored_atomically_with_render_data() {
    let db = Db::open_in_memory().unwrap();
    insert_track(&db);
    let spectrogram = spectrogram([frame(5, 255), frame(7, 255)]);
    let source = TrackSourceFingerprint {
        mtime_seconds: 11,
        size_bytes: 22,
        device: Some(33),
        inode: Some(44),
    };

    assert_eq!(
        set_track_render_data(
            &db,
            1,
            source,
            &TrackRenderData {
                waveform_peaks: vec![1, 2],
                spectrogram: spectrogram.clone(),
            },
        )
        .unwrap(),
        SpectrogramStoreOutcome::Stored
    );
    assert_eq!(
        get_track_sound_features(&db, 1).unwrap(),
        Some(derive_sound_features(&spectrogram))
    );
}

#[test]
fn sound_features_carry_the_temporal_derivation_of_the_same_spectrogram() {
    // The profile is the two halves side by side: whatever the rhythm
    // derivation says about these cells is what the stored profile carries.
    let pulsed = spectrogram(
        (0..=120).map(|index| frame(2, if index > 0 && index % 10 == 0 { 255 } else { 0 })),
    );

    let features = derive_sound_features(&pulsed);

    assert_eq!(features.rhythm, derive_rhythm_features(&pulsed));
    assert_eq!(features.rhythm.onset_rate, 2.0);
    assert_eq!(
        SoundFeatures::from_blob(&features.to_blob()).unwrap(),
        features,
        "the temporal half has to survive the round trip through the blob"
    );
}
