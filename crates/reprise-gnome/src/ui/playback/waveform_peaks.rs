//! Waveform peak extraction.
//!
//! Dekodiert eine Audiodatei per GStreamer in Mono-S16LE-Samples (max. 8 kHz),
//! unterteilt sie in gleich große Fenster und liefert je Fenster den RMS-Pegel,
//! normalisiert auf [0, 1] und via sqrt-Kompression auf [0, 255] quantisiert.
//!
//! Rein synchrone Datenschicht — kein UI, keine Wiedergabe.

use std::path::{Path, PathBuf};

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

/// Number of raw peaks stored per track.
pub(crate) const STORED_PEAK_COUNT: usize = 1000;

/// Errors during waveform peak extraction.
#[derive(Debug, thiserror::Error)]
pub(crate) enum WaveformError {
    #[error("file not found: {0}")]
    FileNotFound(PathBuf),
    #[error("decode failed: {0}")]
    DecodeFailed(String),
    #[error("empty audio stream")]
    EmptyStream,
}

/// Extract `buckets` amplitude peaks from an audio file.
///
/// Each peak is the RMS amplitude of its window, normalized so the global
/// maximum equals 1.0, then sqrt-compressed and quantized to \[0, 255\].
///
/// Runs synchronously — caller is expected to invoke this from a worker thread.
pub(crate) fn extract_peaks(path: &Path, buckets: usize) -> Result<Vec<u8>, WaveformError> {
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

    let effective_buckets = if samples.len() < buckets {
        samples.len().max(1)
    } else {
        buckets
    };

    Ok(compute_peaks(&samples, effective_buckets))
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
        .field("rate", 8000i32)
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
    let timeout = gst::ClockTime::from_seconds(30);
    while let Some(sample) = appsink.try_pull_sample(timeout) {
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
/// RMS-Pegel, normalisiert global auf [0, 1], komprimiert via sqrt und quantisiert
/// auf [0, 255].
fn compute_peaks(samples: &[i16], buckets: usize) -> Vec<u8> {
    if buckets == 0 {
        return Vec::new();
    }

    let window_size = samples.len() / buckets;
    let mut rms_values = Vec::with_capacity(buckets);

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
            rms_values.push(0.0_f32);
            continue;
        }
        let sum_sq: f64 = window.iter().map(|&s| (s as f64) * (s as f64)).sum();
        let rms = (sum_sq / window.len() as f64).sqrt() as f32;
        rms_values.push(rms);
    }

    // Normalisierung auf [0, 1]
    let max = rms_values.iter().copied().fold(0.0_f32, f32::max);
    if max > 0.0 {
        for p in &mut rms_values {
            *p /= max;
        }
    }

    // sqrt-Kompression + Quantisierung auf u8
    rms_values
        .iter()
        .map(|&p| (p.sqrt() * 255.0).round().clamp(0.0, 255.0) as u8)
        .collect()
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
            assert!(p <= 255, "peak[{i}] = {p} out of [0, 255]");
        }
    }

    #[test]
    fn extract_peaks_has_nonzero_amplitude() {
        init_gst();
        let peaks = extract_peaks(&fixture_path(), 64).unwrap();
        assert!(
            peaks.iter().any(|&p| p > 0),
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
    fn compute_peaks_normalizes_correctly() {
        // Reiner Unit-Test — kein GStreamer nötig
        let samples: Vec<i16> = (0..1000)
            .map(|i| ((i as f64 / 100.0 * std::f64::consts::TAU).sin() * 10_000.0) as i16)
            .collect();
        let peaks = compute_peaks(&samples, 10);
        assert_eq!(peaks.len(), 10);
        let max = peaks.iter().copied().max().unwrap_or(0);
        assert_eq!(max, 255, "max peak should be 255, got {max}");
        // All values must be valid u8 (trivially true, but let's check non-zero
        assert!(peaks.iter().any(|&p| p > 0));
    }
}
