use std::io;
use std::sync::{Arc, Mutex};

use reprise_core::device_sync::load_mirror_playlist_snapshots;
use reprise_core::library::loudness::MeasuredLoudness;
use reprise_core::spectrogram::{TrackSourceFingerprint, TrackSpectrogram};
use reprise_core::waveform::TrackRenderData;

use super::*;

#[derive(Clone, Default)]
struct CapturedLog(Arc<Mutex<Vec<u8>>>);

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CapturedLog {
    type Writer = CapturedLogWriter;

    fn make_writer(&'a self) -> Self::Writer {
        CapturedLogWriter(self.0.clone())
    }
}

struct CapturedLogWriter(Arc<Mutex<Vec<u8>>>);

impl io::Write for CapturedLogWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Runs one planning pass over the "Road" playlist and returns everything
/// logged at `DEBUG` and above, together with the planned analyses.
fn plan_road_playlist(
    conn: &Db,
    selected: &[SelectionSource],
) -> (String, Vec<reprise_core::device_sync::DesktopAnalysis>) {
    let captured = CapturedLog::default();
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(captured.clone())
        .finish();
    let playlists = load_mirror_playlist_snapshots(conn).unwrap();
    let analyses = tracing::subscriber::with_default(subscriber, || {
        desktop_analysis_sizes_for_test(conn, selected, &playlists).unwrap()
    });
    let log = String::from_utf8(captured.0.lock().unwrap().clone()).unwrap();
    (log, analyses)
}

fn road_sources() -> Vec<SelectionSource> {
    vec![SelectionSource::Playlist(10)]
}

fn seed_analysis(conn: &Db, track_id: i64) {
    reprise_core::db::set_track_render_data(
        conn,
        track_id,
        TrackSourceFingerprint {
            mtime_seconds: 0,
            size_bytes: 0,
            device: None,
            inode: None,
        },
        &TrackRenderData {
            waveform_peaks: vec![3; 4],
            spectrogram: TrackSpectrogram::from_cells(vec![7; 24]).unwrap(),
            loudness: Some(MeasuredLoudness {
                integrated_lufs: -18.0,
                true_peak: 0.9,
            }),
            decoded_end_ms: None,
        },
    )
    .unwrap();
}

#[test]
fn unanalysed_tracks_are_not_warned_about_one_by_one_during_planning() {
    let (_temp, conn) = fixture();
    select_road_playlist(&conn, &[1, 2, 3]);

    let (log, analyses) = plan_road_playlist(&conn, &road_sources());

    assert!(analyses.is_empty(), "{log}");
    assert!(!log.contains("WARN"), "{log}");
    assert!(!log.contains("track_id"), "{log}");
}

#[test]
fn unanalysed_tracks_are_summed_up_in_one_debug_line_per_planning_pass() {
    let (_temp, conn) = fixture();
    select_road_playlist(&conn, &[1, 2, 3]);

    let (log, _) = plan_road_playlist(&conn, &road_sources());

    let summaries = log
        .lines()
        .filter(|line| line.contains("selected tracks have no analysis sidecar"))
        .collect::<Vec<_>>();
    assert_eq!(summaries.len(), 1, "{log}");
    assert!(summaries[0].contains("DEBUG"), "{log}");
    assert!(summaries[0].contains("unanalysed=3"), "{log}");
    assert!(summaries[0].contains("selected_tracks=3"), "{log}");
}

#[test]
fn a_fully_analysed_selection_plans_a_sidecar_without_a_summary_or_warning() {
    let (_temp, conn) = fixture();
    select_road_playlist(&conn, &[1]);
    seed_analysis(&conn, 1);

    let (log, analyses) = plan_road_playlist(&conn, &road_sources());

    assert_eq!(analyses.len(), 1, "{log}");
    assert_eq!(analyses[0].track_id, 1, "{log}");
    assert!(analyses[0].size_bytes > 0, "{log}");
    assert!(!log.contains("analysis sidecar"), "{log}");
    assert!(!log.contains("WARN"), "{log}");
}

#[test]
fn sidecar_load_errors_surface_as_one_warning_per_planning_pass() {
    let (_temp, conn) = fixture();
    select_road_playlist(&conn, &[1, 2, 3]);
    crate::test_db::connection(conn.as_ref())
        .execute("DROP TABLE track_spectrograms", [])
        .unwrap();

    let (log, analyses) = plan_road_playlist(&conn, &road_sources());

    assert!(analyses.is_empty(), "{log}");
    let warnings = log
        .lines()
        .filter(|line| line.contains("WARN"))
        .collect::<Vec<_>>();
    assert_eq!(warnings.len(), 1, "{log}");
    assert!(
        warnings[0].contains("could not load analysis sidecar data"),
        "{log}"
    );
    assert!(warnings[0].contains("failed_tracks=3"), "{log}");
    assert!(warnings[0].contains("example_track_id=1"), "{log}");
    assert!(warnings[0].contains("track_spectrograms"), "{log}");
    assert!(!log.contains("track_id=2"), "{log}");
    assert!(!log.contains("track_id=3"), "{log}");
}

#[test]
fn analysed_unanalysed_and_failed_tracks_keep_separate_counters() {
    let (_temp, conn) = fixture();
    select_road_playlist(&conn, &[1, 2, 3]);
    seed_analysis(&conn, 2);
    seed_analysis(&conn, 3);
    crate::test_db::connection(conn.as_ref())
        .execute(
            "UPDATE track_spectrograms SET data = 'abcdefghijklmnopqrstuvwx' WHERE track_id = 3",
            [],
        )
        .unwrap();

    let (log, analyses) = plan_road_playlist(&conn, &road_sources());

    assert_eq!(analyses.len(), 1, "{log}");
    assert_eq!(analyses[0].track_id, 2, "{log}");
    assert!(analyses[0].size_bytes > 0, "{log}");
    assert!(log.contains("unanalysed=1"), "{log}");
    assert!(log.contains("selected_tracks=3"), "{log}");
    assert!(log.contains("failed_tracks=1"), "{log}");
    assert!(log.contains("example_track_id=3"), "{log}");
}
