//! Waveform peak extraction and file-based caching.
//!
//! Dekodiert eine Audiodatei per GStreamer in Mono-S16LE-Samples, unterteilt
//! sie in gleich große Fenster und liefert je Fenster den RMS-Pegel, normalisiert
//! auf [0, 1]. Ergebnisse werden dateibasiert unter
//! `glib::user_cache_dir()/reprise/waveforms/` zwischengespeichert.
//!
//! Rein synchrone Datenschicht — kein UI, keine Wiedergabe.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

/// Errors during waveform peak extraction or cache I/O.
#[derive(Debug, thiserror::Error)]
pub(crate) enum WaveformError {
    #[error("file not found: {0}")]
    FileNotFound(PathBuf),
    #[error("decode failed: {0}")]
    DecodeFailed(String),
    #[error("empty audio stream")]
    EmptyStream,
    #[error("cache I/O: {0}")]
    CacheIo(#[from] std::io::Error),
}

/// Extract `buckets` normalized amplitude peaks from an audio file.
///
/// Each peak is the RMS amplitude of its window, normalized so the global
/// maximum equals 1.0. All values lie in \[0.0, 1.0\].
///
/// Runs synchronously — caller is expected to invoke this from a worker thread.
pub(crate) fn extract_peaks(path: &Path, buckets: usize) -> Result<Vec<f32>, WaveformError> {
    if !path.exists() {
        return Err(WaveformError::FileNotFound(path.to_path_buf()));
    }

    gst::init().map_err(|e| WaveformError::DecodeFailed(format!("GStreamer init: {e}")))?;

    let (pipeline, appsink) = build_decode_pipeline(path)?;

    pipeline
        .set_state(gst::State::Playing)
        .map_err(|e| WaveformError::DecodeFailed(format!("set state: {e}")))?;

    let samples = pull_all_samples(&appsink);

    // Prüfe den GStreamer-Bus auf Dekodierungsfehler vor dem Teardown.
    check_bus_errors(&pipeline)?;

    pipeline.set_state(gst::State::Null).ok();

    let samples = samples?;
    if samples.is_empty() {
        return Err(WaveformError::EmptyStream);
    }

    Ok(compute_peaks(&samples, buckets))
}

/// Load cached peaks or extract + cache them (default cache directory).
pub(crate) fn cached_peaks(path: &Path, buckets: usize) -> Result<Vec<f32>, WaveformError> {
    let cache_dir = gtk4::glib::user_cache_dir().join("reprise/waveforms");
    cached_peaks_in(path, buckets, &cache_dir)
}

/// Testbare Variante mit explizitem Cache-Verzeichnis.
fn cached_peaks_in(
    path: &Path,
    buckets: usize,
    cache_dir: &Path,
) -> Result<Vec<f32>, WaveformError> {
    let cache_path = cache_file_path(path, buckets, cache_dir)?;

    if let Some(peaks) = try_load_cache(&cache_path, buckets) {
        return Ok(peaks);
    }

    let peaks = extract_peaks(path, buckets)?;
    write_cache(&cache_path, &peaks)?;
    Ok(peaks)
}

// ---------------------------------------------------------------------------
// GStreamer-Pipeline
// ---------------------------------------------------------------------------

fn build_decode_pipeline(path: &Path) -> Result<(gst::Pipeline, gst_app::AppSink), WaveformError> {
    let path_str = path
        .to_str()
        .ok_or_else(|| WaveformError::DecodeFailed("path is not valid UTF-8".into()))?;

    let pipeline = gst::Pipeline::default();

    let filesrc = make_element("filesrc")?;
    filesrc.set_property("location", path_str);

    let decodebin = make_element("decodebin")?;
    let audioconvert = make_element("audioconvert")?;
    let audioresample = make_element("audioresample")?;

    let caps = gst::Caps::builder("audio/x-raw")
        .field("format", "S16LE")
        .field("channels", 1i32)
        .build();
    let capsfilter = make_element("capsfilter")?;
    capsfilter.set_property("caps", &caps);

    let appsink_el = make_element("appsink")?;

    for el in [
        &filesrc,
        &decodebin,
        &audioconvert,
        &audioresample,
        &capsfilter,
        &appsink_el,
    ] {
        pipeline
            .add(el)
            .map_err(|e| WaveformError::DecodeFailed(format!("add element: {e}")))?;
    }

    // Statische Links
    filesrc
        .link(&decodebin)
        .map_err(|e| WaveformError::DecodeFailed(format!("link filesrc→decodebin: {e}")))?;
    audioconvert
        .link(&audioresample)
        .map_err(|e| WaveformError::DecodeFailed(format!("link convert→resample: {e}")))?;
    audioresample
        .link(&capsfilter)
        .map_err(|e| WaveformError::DecodeFailed(format!("link resample→capsfilter: {e}")))?;
    capsfilter
        .link(&appsink_el)
        .map_err(|e| WaveformError::DecodeFailed(format!("link capsfilter→appsink: {e}")))?;

    // Dynamischer Link: decodebin → audioconvert (Pad entsteht erst beim Dekodieren)
    let convert_weak = audioconvert.downgrade();
    decodebin.connect_pad_added(move |_el, src_pad| {
        let Some(convert) = convert_weak.upgrade() else {
            return;
        };
        let Some(sink_pad) = convert.static_pad("sink") else {
            return;
        };
        if sink_pad.is_linked() {
            return;
        }
        src_pad.link(&sink_pad).ok();
    });

    let appsink = appsink_el
        .downcast::<gst_app::AppSink>()
        .map_err(|_| WaveformError::DecodeFailed("appsink downcast failed".into()))?;

    Ok((pipeline, appsink))
}

fn make_element(factory_name: &str) -> Result<gst::Element, WaveformError> {
    gst::ElementFactory::make(factory_name)
        .build()
        .map_err(|e| WaveformError::DecodeFailed(format!("{factory_name}: {e}")))
}

fn pull_all_samples(appsink: &gst_app::AppSink) -> Result<Vec<i16>, WaveformError> {
    let mut samples = Vec::new();
    while let Ok(sample) = appsink.pull_sample() {
        let buffer = sample
            .buffer()
            .ok_or_else(|| WaveformError::DecodeFailed("sample without buffer".into()))?;
        let map = buffer
            .map_readable()
            .map_err(|e| WaveformError::DecodeFailed(format!("buffer map: {e}")))?;
        // S16LE: 2 Bytes pro Sample
        for chunk in map.as_slice().chunks_exact(2) {
            samples.push(i16::from_le_bytes([chunk[0], chunk[1]]));
        }
    }
    Ok(samples)
}

fn check_bus_errors(pipeline: &gst::Pipeline) -> Result<(), WaveformError> {
    let Some(bus) = pipeline.bus() else {
        return Ok(());
    };
    while let Some(msg) = bus.pop() {
        if let gst::MessageView::Error(err) = msg.view() {
            return Err(WaveformError::DecodeFailed(err.error().to_string()));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Peak-Berechnung
// ---------------------------------------------------------------------------

/// Teilt die Samples in `buckets` gleich große Fenster, berechnet je Fenster den
/// RMS-Pegel und normalisiert global auf [0, 1].
fn compute_peaks(samples: &[i16], buckets: usize) -> Vec<f32> {
    if buckets == 0 {
        return Vec::new();
    }

    let window_size = samples.len() / buckets;
    let mut peaks = Vec::with_capacity(buckets);

    for i in 0..buckets {
        let start = i * window_size;
        // Letztes Fenster bekommt ggf. mehr Samples (Restverteilung).
        let end = if i == buckets - 1 {
            samples.len()
        } else {
            start + window_size
        };
        let window = &samples[start..end];
        if window.is_empty() {
            peaks.push(0.0);
            continue;
        }
        let sum_sq: f64 = window.iter().map(|&s| (s as f64) * (s as f64)).sum();
        let rms = (sum_sq / window.len() as f64).sqrt() as f32;
        peaks.push(rms);
    }

    // Normalisierung auf [0, 1]
    let max = peaks.iter().copied().fold(0.0_f32, f32::max);
    if max > 0.0 {
        for p in &mut peaks {
            *p /= max;
        }
    }

    peaks
}

// ---------------------------------------------------------------------------
// Datei-Cache
// ---------------------------------------------------------------------------

/// Berechnet den Cache-Dateipfad: Hash aus Pfad + mtime + Bucket-Anzahl.
fn cache_file_path(
    path: &Path,
    buckets: usize,
    cache_dir: &Path,
) -> Result<PathBuf, WaveformError> {
    let metadata =
        fs::metadata(path).map_err(|_| WaveformError::FileNotFound(path.to_path_buf()))?;
    let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);

    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    mtime.hash(&mut hasher);
    buckets.hash(&mut hasher);
    let hash = hasher.finish();

    fs::create_dir_all(cache_dir)?;
    Ok(cache_dir.join(format!("{hash:016x}.peaks")))
}

/// Versucht, gecachte Peaks zu laden. Gibt `None` bei fehlender/kaputter Datei.
fn try_load_cache(cache_path: &Path, expected_len: usize) -> Option<Vec<f32>> {
    let data = fs::read(cache_path).ok()?;
    // Format: u32 LE (Anzahl), dann Anzahl × f32 LE.
    if data.len() < 4 {
        return None;
    }
    let count = u32::from_le_bytes(data[0..4].try_into().ok()?) as usize;
    if count != expected_len || data.len() != 4 + count * 4 {
        return None;
    }
    let mut peaks = Vec::with_capacity(count);
    for i in 0..count {
        let off = 4 + i * 4;
        peaks.push(f32::from_le_bytes(data[off..off + 4].try_into().ok()?));
    }
    Some(peaks)
}

fn write_cache(cache_path: &Path, peaks: &[f32]) -> Result<(), WaveformError> {
    let mut data = Vec::with_capacity(4 + peaks.len() * 4);
    data.extend_from_slice(&(peaks.len() as u32).to_le_bytes());
    for &v in peaks {
        data.extend_from_slice(&v.to_le_bytes());
    }
    fs::write(cache_path, &data)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Once;

    static GST_INIT: Once = Once::new();

    fn init_gst() {
        GST_INIT.call_once(|| {
            gst::init().unwrap();
        });
    }

    fn fixture_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../reprise-core/tests/fixtures/sine.flac")
    }

    #[test]
    fn extract_peaks_returns_exact_bucket_count() {
        init_gst();
        let peaks = extract_peaks(&fixture_path(), 64).unwrap();
        assert_eq!(peaks.len(), 64);
    }

    #[test]
    fn extract_peaks_values_in_unit_range() {
        init_gst();
        let peaks = extract_peaks(&fixture_path(), 64).unwrap();
        for (i, &p) in peaks.iter().enumerate() {
            assert!(
                (0.0..=1.0).contains(&p),
                "peak[{i}] = {p} out of [0.0, 1.0]"
            );
        }
    }

    #[test]
    fn extract_peaks_has_nonzero_amplitude() {
        init_gst();
        let peaks = extract_peaks(&fixture_path(), 64).unwrap();
        assert!(
            peaks.iter().any(|&p| p > 0.0),
            "sine wave must have positive amplitudes"
        );
    }

    #[test]
    fn extract_peaks_is_deterministic() {
        init_gst();
        let path = fixture_path();
        let a = extract_peaks(&path, 64).unwrap();
        let b = extract_peaks(&path, 64).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn extract_peaks_errors_on_missing_file() {
        init_gst();
        let result = extract_peaks(Path::new("/tmp/nonexistent_waveform_test.flac"), 64);
        assert!(result.is_err());
    }

    #[test]
    fn cached_peaks_creates_and_reuses_cache() {
        init_gst();
        let tmp = tempfile::tempdir().unwrap();
        let path = fixture_path();

        let first = cached_peaks_in(&path, 64, tmp.path()).unwrap();

        // Cache-Datei muss existieren
        let cache_path = cache_file_path(&path, 64, tmp.path()).unwrap();
        assert!(cache_path.exists(), "cache file must be created");

        // Zweiter Aufruf liefert identische Werte (aus dem Cache)
        let second = cached_peaks_in(&path, 64, tmp.path()).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn mtime_change_invalidates_cache() {
        init_gst();
        let tmp_cache = tempfile::tempdir().unwrap();
        let tmp_file = tempfile::tempdir().unwrap();

        let src = fixture_path();
        let dest = tmp_file.path().join("sine.flac");
        fs::copy(&src, &dest).unwrap();

        let first = cached_peaks_in(&dest, 64, tmp_cache.path()).unwrap();
        let cache_path_1 = cache_file_path(&dest, 64, tmp_cache.path()).unwrap();
        assert!(cache_path_1.exists());

        // mtime vorstellen → anderer Hash → anderer Cache-Pfad
        let file = fs::File::options().write(true).open(&dest).unwrap();
        let future = SystemTime::now() + std::time::Duration::from_secs(100);
        file.set_times(fs::FileTimes::new().set_modified(future))
            .unwrap();
        drop(file);

        let cache_path_2 = cache_file_path(&dest, 64, tmp_cache.path()).unwrap();
        assert_ne!(
            cache_path_1, cache_path_2,
            "mtime change must produce a different cache key"
        );

        let second = cached_peaks_in(&dest, 64, tmp_cache.path()).unwrap();
        assert_eq!(first, second); // Gleiche Audiodaten → gleiche Peaks
        assert!(cache_path_2.exists()); // Neue Cache-Datei angelegt
    }

    #[test]
    fn corrupt_cache_is_regenerated() {
        init_gst();
        let tmp = tempfile::tempdir().unwrap();
        let path = fixture_path();

        // Kaputte Datei an die Cache-Position schreiben
        let cache_path = cache_file_path(&path, 64, tmp.path()).unwrap();
        fs::write(&cache_path, b"garbage").unwrap();

        // Muss trotz kaputter Cache-Datei korrekt extrahieren
        let peaks = cached_peaks_in(&path, 64, tmp.path()).unwrap();
        assert_eq!(peaks.len(), 64);
        assert!(peaks.iter().any(|&p| p > 0.0));
    }

    #[test]
    fn compute_peaks_normalizes_correctly() {
        // Reiner Unit-Test — kein GStreamer nötig
        let samples: Vec<i16> = (0..1000)
            .map(|i| ((i as f64 / 100.0 * std::f64::consts::TAU).sin() * 10_000.0) as i16)
            .collect();
        let peaks = compute_peaks(&samples, 10);
        assert_eq!(peaks.len(), 10);
        let max = peaks.iter().copied().fold(0.0_f32, f32::max);
        assert!(
            (max - 1.0).abs() < f32::EPSILON,
            "max peak should be 1.0, got {max}"
        );
        assert!(peaks.iter().all(|&p| (0.0..=1.0).contains(&p)));
    }
}
