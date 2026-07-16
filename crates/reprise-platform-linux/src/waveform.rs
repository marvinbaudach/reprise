//! GStreamer-backed waveform extraction for Linux frontends.

use std::path::Path;
use std::process::{Command, Stdio};

use reprise_core::waveform::{WaveformBackend, WaveformError};

#[derive(Clone, Copy, Default)]
pub struct GstreamerWaveformBackend;

impl WaveformBackend for GstreamerWaveformBackend {
    fn extract_peaks(&self, path: &Path, buckets: usize) -> Result<Vec<u8>, WaveformError> {
        extract_peaks(path, buckets)
    }
}

fn extract_peaks(path: &Path, buckets: usize) -> Result<Vec<u8>, WaveformError> {
    if !path.exists() {
        return Err(WaveformError::FileNotFound(path.to_path_buf()));
    }
    gstreamer::init()
        .map_err(|error| WaveformError::DecodeFailed(format!("GStreamer init: {error}")))?;

    let output = Command::new("gst-launch-1.0")
        .arg("filesrc")
        .arg(format!("location={}", path.to_string_lossy()))
        .arg("!")
        .arg("decodebin")
        .arg("!")
        .arg("audioconvert")
        .arg("!")
        .arg("audioresample")
        .arg("!")
        .arg("audio/x-raw,format=S16LE,channels=1,rate=8000")
        .arg("!")
        .arg("fdsink")
        .arg("fd=1")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .map_err(|error| WaveformError::DecodeFailed(format!("gst-launch: {error}")))?;

    if !output.status.success() {
        return Err(WaveformError::DecodeFailed(format!(
            "gst-launch exit code: {:?}",
            output.status.code()
        )));
    }

    let samples: Vec<i16> = output
        .stdout
        .chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    if samples.is_empty() {
        return Err(WaveformError::EmptyStream);
    }
    Ok(compute_peaks(&samples, buckets.min(samples.len())))
}

fn compute_peaks(samples: &[i16], buckets: usize) -> Vec<u8> {
    if buckets == 0 {
        return Vec::new();
    }
    let window_size = samples.len() / buckets;
    let mut rms_values = Vec::with_capacity(buckets);
    for index in 0..buckets {
        let start = index * window_size;
        let end = if index == buckets - 1 {
            samples.len()
        } else {
            start + window_size
        };
        let window = &samples[start..end];
        if window.is_empty() {
            rms_values.push(0.0_f32);
            continue;
        }
        let sum_sq: f64 = window
            .iter()
            .map(|&sample| f64::from(sample) * f64::from(sample))
            .sum();
        rms_values.push((sum_sq / window.len() as f64).sqrt() as f32);
    }
    let max = rms_values.iter().copied().fold(0.0_f32, f32::max);
    if max > 0.0 {
        for peak in &mut rms_values {
            *peak /= max;
        }
    }
    rms_values
        .iter()
        .map(|peak| (peak.sqrt() * 255.0).round().clamp(0.0, 255.0) as u8)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use reprise_core::waveform::WaveformBackend;

    use super::*;

    fn fixture_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../reprise-core/tests/fixtures/sine.flac")
    }

    #[test]
    fn extract_peaks_returns_requested_bucket_count() {
        let peaks = GstreamerWaveformBackend
            .extract_peaks(&fixture_path(), 64)
            .unwrap();
        assert_eq!(peaks.len(), 64);
    }

    #[test]
    fn extract_peaks_has_nonzero_amplitude() {
        let peaks = GstreamerWaveformBackend
            .extract_peaks(&fixture_path(), 64)
            .unwrap();
        assert!(peaks.iter().any(|peak| *peak > 0));
    }

    #[test]
    fn extract_peaks_errors_on_missing_file() {
        let result = GstreamerWaveformBackend
            .extract_peaks(Path::new("/tmp/nonexistent_waveform_test.flac"), 64);
        assert!(result.is_err());
    }

    #[test]
    fn peak_math_is_deterministic() {
        let samples: Vec<i16> = (0..5000)
            .map(|index| ((index as f64 / 73.0 * std::f64::consts::TAU).sin() * 12_000.0) as i16)
            .collect();
        let first = compute_peaks(&samples, 64);
        assert_eq!(first, compute_peaks(&samples, 64));
    }

    #[test]
    fn peak_math_is_normalized_and_handles_zero_buckets() {
        let samples: Vec<i16> = (0..1000)
            .map(|index| ((index as f64 / 100.0 * std::f64::consts::TAU).sin() * 10_000.0) as i16)
            .collect();
        let first = compute_peaks(&samples, 10);
        assert_eq!(first.iter().copied().max(), Some(255));
        assert!(compute_peaks(&samples, 0).is_empty());
    }
}
