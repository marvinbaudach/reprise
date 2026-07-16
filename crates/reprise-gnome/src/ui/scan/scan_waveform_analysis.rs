//! Off-thread waveform extraction used after scans and for startup backfill.

use std::sync::Arc;

use reprise_core::waveform::{WaveformBackend, STORED_PEAK_COUNT};

/// How many platform waveform extractions to run in parallel.
const WAVEFORM_WORKERS: usize = 4;

/// Analyzes waveform peaks for all tracks that don't have them yet.
/// Parallelizes across `WAVEFORM_WORKERS` threads, each with its own DB
/// connection. Uses a shared work queue (atomic index into the track list).
pub(super) fn analyze_waveforms(db_path: &std::path::Path, waveform_backend: &dyn WaveformBackend) {
    let conn = match reprise_core::db::open_migrated(Some(db_path)) {
        Ok(c) => c,
        Err(_) => return,
    };
    let tracks = match reprise_core::db::pending_waveform_tracks(&conn) {
        Ok(tracks) => tracks,
        Err(_) => return,
    };
    drop(conn);

    if tracks.is_empty() {
        tracing::info!("waveform backfill: all tracks already analyzed");
        return;
    }
    let total = tracks.len();
    tracing::info!(
        total,
        workers = WAVEFORM_WORKERS,
        "waveform backfill: starting"
    );

    let cursor = std::sync::atomic::AtomicUsize::new(0);
    let done = std::sync::atomic::AtomicUsize::new(0);
    let failed = std::sync::atomic::AtomicUsize::new(0);

    std::thread::scope(|scope| {
        for _ in 0..WAVEFORM_WORKERS {
            scope.spawn(|| {
                let Ok(worker_conn) = reprise_core::db::open_migrated(Some(db_path)) else {
                    return;
                };
                loop {
                    let idx = cursor.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if idx >= total {
                        break;
                    }
                    let (track_id, ref path_str) = tracks[idx];
                    let path = std::path::Path::new(path_str);
                    match waveform_backend.extract_peaks(path, STORED_PEAK_COUNT) {
                        Ok(peaks) => {
                            if reprise_core::db::set_waveform_peaks(&worker_conn, track_id, &peaks)
                                .is_ok()
                            {
                                done.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            } else {
                                failed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            }
                        }
                        Err(_) => {
                            failed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                    let progress = done.load(std::sync::atomic::Ordering::Relaxed)
                        + failed.load(std::sync::atomic::Ordering::Relaxed);
                    if progress.is_multiple_of(100) {
                        tracing::info!(
                            done = done.load(std::sync::atomic::Ordering::Relaxed),
                            failed = failed.load(std::sync::atomic::Ordering::Relaxed),
                            total,
                            "waveform backfill progress"
                        );
                    }
                }
            });
        }
    });

    tracing::info!(
        done = done.load(std::sync::atomic::Ordering::Relaxed),
        failed = failed.load(std::sync::atomic::Ordering::Relaxed),
        total,
        "waveform backfill complete"
    );
}

/// Spawns a background thread that analyzes waveform peaks for all tracks
/// without peaks in the DB. Called once at app startup so existing libraries
/// get peaks without requiring a manual rescan.
pub(super) fn spawn_waveform_backfill(
    db_path: std::path::PathBuf,
    waveform_backend: Arc<dyn WaveformBackend>,
) {
    std::thread::Builder::new()
        .name("reprise-waveform-backfill".to_string())
        .spawn(move || {
            analyze_waveforms(&db_path, waveform_backend.as_ref());
        })
        .ok();
}
