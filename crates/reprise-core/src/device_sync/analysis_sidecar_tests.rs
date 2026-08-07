use std::path::Path;

use crate::spectrogram::{TrackSourceFingerprint, TrackSpectrogram};

use super::*;

fn source() -> TrackSourceFingerprint {
    TrackSourceFingerprint {
        mtime_seconds: 1_725_000_123,
        size_bytes: 9_876_543,
        device: Some(41),
        inode: Some(73),
    }
}

#[test]
fn analysis_sidecar_round_trips_its_source_fingerprint_and_render_data() {
    let sidecar = AnalysisSidecar::new(
        source(),
        TrackSpectrogram::from_cells(vec![7; 48]).unwrap(),
        vec![3, 5, 8, 13],
    );

    let encoded = sidecar.encode().unwrap();

    assert_eq!(&encoded[..8], b"RPA-SIDE");
    assert_eq!(u16::from_le_bytes([encoded[8], encoded[9]]), FORMAT_VERSION);
    assert_eq!(AnalysisSidecar::decode(&encoded).unwrap(), sidecar);
}

#[test]
fn analysis_sidecar_rejects_a_recognisable_future_version() {
    let mut encoded = AnalysisSidecar::new(source(), TrackSpectrogram::empty(), vec![1])
        .encode()
        .unwrap();
    encoded[8..10].copy_from_slice(&2_u16.to_le_bytes());

    assert_eq!(
        AnalysisSidecar::decode(&encoded),
        Err(AnalysisSidecarError::UnsupportedVersion(2))
    );
}

#[test]
fn analysis_sidecar_reports_an_invalid_optional_field_tag_separately_from_magic() {
    let mut encoded = AnalysisSidecar::new(source(), TrackSpectrogram::empty(), vec![1])
        .encode()
        .unwrap();
    encoded[26] = 7;

    assert_eq!(
        AnalysisSidecar::decode(&encoded).unwrap_err().to_string(),
        "analysis sidecar optional field has invalid tag 7"
    );
}

#[test]
fn analysis_sidecar_path_follows_the_transcoded_audio_name() {
    assert_eq!(
        device_path_for_track("Artist/Album/01 Song.opus"),
        Some("Artist/Album/01 Song.reprise-analysis".into())
    );
    assert!(is_sidecar_path(Path::new(
        "Artist/Album/01 Song.REPRISE-ANALYSIS"
    )));
    assert!(!is_sidecar_path(Path::new("Artist/Album/01 Song.opus")));
}

#[test]
fn analysis_sidecar_for_track_uses_the_database_source_fingerprint() {
    let db = crate::db::Db::open_in_memory().unwrap();
    db.conn()
        .execute(
            "INSERT INTO tracks \
             (id, path, title, added_at, file_mtime, file_size, device, inode) \
             VALUES (7, '/library/song.flac', 'Song', 0, ?1, ?2, ?3, ?4)",
            rusqlite::params![
                source().mtime_seconds,
                source().size_bytes,
                source().device,
                source().inode
            ],
        )
        .unwrap();
    let render_data = crate::waveform::TrackRenderData {
        waveform_peaks: vec![2, 4, 6],
        spectrogram: TrackSpectrogram::from_cells(vec![9; 24]).unwrap(),
    };
    crate::db_spectrogram::set_track_render_data(&db, 7, source(), &render_data).unwrap();

    let sidecar = AnalysisSidecar::for_track(&db, 7).unwrap().unwrap();

    assert_eq!(sidecar.source, source());
    assert_eq!(sidecar.waveform_peaks, vec![2, 4, 6]);
    assert_eq!(sidecar.spectrogram.cells(), &[9; 24]);
}
