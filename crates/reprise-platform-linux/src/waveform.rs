//! GStreamer-backed waveform extraction for Linux frontends.
//!
//! A bounded `uridecodebin` pipeline decodes to calibrated 8 kHz mono F32 PCM
//! and streams it through a [`WaveformAccumulator`], which reduces it to the
//! byte peaks the seek bar renders. Decoding is memory-bounded (a small queue
//! of buffers) and cancellable between pulled samples.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use reprise_core::waveform::{WaveformAccumulator, WaveformBackend, WaveformError};

const SAMPLE_RATE: u32 = 8_000;
const PIPELINE_DESCRIPTION: &str = "uridecodebin name=decoder ! audioconvert ! audioresample ! \
    audio/x-raw,format=F32LE,channels=1,rate=8000,layout=interleaved ! \
    appsink name=sink sync=false";
const STATE_TIMEOUT: gst::ClockTime = gst::ClockTime::from_seconds(5);
const PULL_TIMEOUT: gst::ClockTime = gst::ClockTime::from_mseconds(50);
const MAX_QUEUED_BUFFERS: u32 = 2;
const NANOSECONDS_PER_SECOND: u64 = 1_000_000_000;

#[derive(Clone, Copy, Default)]
pub struct GstreamerWaveformBackend;

impl WaveformBackend for GstreamerWaveformBackend {
    fn extract_peaks(&self, path: &Path, buckets: usize) -> Result<Vec<u8>, WaveformError> {
        if buckets == 0 {
            return Ok(Vec::new());
        }
        extract(path, buckets, &AtomicBool::new(false))
    }

    fn extract_peaks_cancellable(
        &self,
        path: &Path,
        buckets: usize,
        cancelled: &AtomicBool,
    ) -> Result<Vec<u8>, WaveformError> {
        if buckets == 0 {
            return Ok(Vec::new());
        }
        extract(path, buckets, cancelled)
    }
}

fn extract(path: &Path, buckets: usize, cancelled: &AtomicBool) -> Result<Vec<u8>, WaveformError> {
    if !path.is_file() {
        return Err(WaveformError::FileNotFound(path.to_path_buf()));
    }
    if cancelled.load(Ordering::Acquire) {
        return Err(WaveformError::Cancelled);
    }
    gst::init().map_err(|error| WaveformError::DecodeFailed(error.to_string()))?;
    let (pipeline, sink) = build_pipeline(path)?;
    let result = run_pipeline(&pipeline, &sink, cancelled, buckets);
    let _ = pipeline.set_state(gst::State::Null);
    result
}

fn build_pipeline(path: &Path) -> Result<(gst::Pipeline, gst_app::AppSink), WaveformError> {
    let uri = gst::glib::filename_to_uri(path, None)
        .map_err(|_| WaveformError::DecodeFailed("path cannot be converted to URI".into()))?;
    let pipeline = gst::parse::launch(PIPELINE_DESCRIPTION)
        .map_err(|error| WaveformError::DecodeFailed(error.to_string()))?
        .downcast::<gst::Pipeline>()
        .map_err(|_| WaveformError::DecodeFailed("parser did not create a pipeline".into()))?;
    pipeline
        .by_name("decoder")
        .ok_or_else(|| WaveformError::DecodeFailed("pipeline has no decoder".into()))?
        .set_property("uri", uri.to_string());
    let sink = pipeline
        .by_name("sink")
        .ok_or_else(|| WaveformError::DecodeFailed("pipeline has no AppSink".into()))?
        .downcast::<gst_app::AppSink>()
        .map_err(|_| WaveformError::DecodeFailed("sink is not an AppSink".into()))?;
    sink.set_max_buffers(MAX_QUEUED_BUFFERS);
    sink.set_drop(false);
    sink.set_wait_on_eos(false);
    Ok((pipeline, sink))
}

fn run_pipeline(
    pipeline: &gst::Pipeline,
    sink: &gst_app::AppSink,
    cancelled: &AtomicBool,
    buckets: usize,
) -> Result<Vec<u8>, WaveformError> {
    pipeline
        .set_state(gst::State::Paused)
        .map_err(|error| WaveformError::DecodeFailed(error.to_string()))?;
    let (state_result, _, _) = pipeline.state(STATE_TIMEOUT);
    state_result.map_err(|error| WaveformError::DecodeFailed(error.to_string()))?;
    let duration = pipeline
        .query_duration::<gst::ClockTime>()
        .ok_or_else(|| WaveformError::DecodeFailed("stream duration is unavailable".into()))?;
    let expected_samples = duration
        .nseconds()
        .saturating_mul(u64::from(SAMPLE_RATE))
        // Container duration is commonly fractional after resampling. It is
        // an upper-bound capacity here, not a nearest-sample measurement: a
        // one-sample underestimate would reject an otherwise valid stream.
        .saturating_add(NANOSECONDS_PER_SECOND - 1)
        / NANOSECONDS_PER_SECOND;
    if expected_samples == 0 {
        return Err(WaveformError::EmptyStream);
    }
    let mut accumulator = WaveformAccumulator::new(expected_samples, buckets)?;
    pipeline
        .set_state(gst::State::Playing)
        .map_err(|error| WaveformError::DecodeFailed(error.to_string()))?;
    let bus = pipeline
        .bus()
        .ok_or_else(|| WaveformError::DecodeFailed("pipeline has no bus".into()))?;
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Err(WaveformError::Cancelled);
        }
        if let Some(sample) = sink.try_pull_sample(PULL_TIMEOUT) {
            push_sample(&mut accumulator, &sample)?;
            continue;
        }
        if let Some(message) = bus.timed_pop_filtered(
            gst::ClockTime::ZERO,
            &[gst::MessageType::Eos, gst::MessageType::Error],
        ) {
            match message.view() {
                gst::MessageView::Eos(_) => break,
                gst::MessageView::Error(error) => {
                    return Err(WaveformError::DecodeFailed(format!(
                        "{} ({:?})",
                        error.error(),
                        error.debug()
                    )));
                }
                _ => {}
            }
        }
        if sink.is_eos() {
            break;
        }
    }
    accumulator.finish()
}

fn push_sample(
    accumulator: &mut WaveformAccumulator,
    sample: &gst::Sample,
) -> Result<(), WaveformError> {
    let buffer = sample
        .buffer()
        .ok_or_else(|| WaveformError::DecodeFailed("sample has no buffer".into()))?;
    let map = buffer
        .map_readable()
        .map_err(|_| WaveformError::DecodeFailed("sample buffer is unreadable".into()))?;
    let bytes = map.as_slice();
    if !bytes.len().is_multiple_of(size_of::<f32>()) {
        return Err(WaveformError::DecodeFailed(
            "sample buffer is not aligned F32 audio".into(),
        ));
    }
    let samples = bytes
        .chunks_exact(size_of::<f32>())
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect::<Vec<_>>();
    accumulator.push(&samples)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;

    use reprise_core::waveform::{WaveformBackend, WaveformError};

    use super::*;

    fn flac_fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac")
    }

    fn mp3_fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.mp3")
    }

    fn write_wav(path: &Path, samples: &[i16]) {
        let data_size = u32::try_from(std::mem::size_of_val(samples)).unwrap();
        let mut wav = Vec::with_capacity(44 + data_size as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_size).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
        wav.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
        wav.extend_from_slice(&2_u16.to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_size.to_le_bytes());
        for sample in samples {
            wav.extend_from_slice(&sample.to_le_bytes());
        }
        fs::write(path, wav).unwrap();
    }

    #[test]
    fn extract_peaks_returns_requested_bucket_count() {
        let peaks = GstreamerWaveformBackend
            .extract_peaks(&flac_fixture(), 64)
            .unwrap();
        assert_eq!(peaks.len(), 64);
    }

    #[test]
    fn extract_peaks_has_nonzero_amplitude() {
        let peaks = GstreamerWaveformBackend
            .extract_peaks(&flac_fixture(), 64)
            .unwrap();
        assert!(peaks.iter().any(|peak| *peak > 0));
    }

    #[test]
    fn extraction_is_deterministic_and_zero_buckets_are_empty() {
        let first = GstreamerWaveformBackend
            .extract_peaks(&flac_fixture(), 64)
            .unwrap();
        let second = GstreamerWaveformBackend
            .extract_peaks(&flac_fixture(), 64)
            .unwrap();

        assert_eq!(first, second);
        assert_eq!(first.iter().copied().max(), Some(255));
        assert!(GstreamerWaveformBackend
            .extract_peaks(&flac_fixture(), 0)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn extract_peaks_errors_on_missing_file() {
        let missing = Path::new("/tmp/reprise-waveform-missing.flac");
        let result = GstreamerWaveformBackend.extract_peaks(missing, 64);
        assert!(matches!(result, Err(WaveformError::FileNotFound(path)) if path == missing));
    }

    #[test]
    fn pre_cancelled_extraction_never_starts() {
        let cancelled = AtomicBool::new(true);
        let result =
            GstreamerWaveformBackend.extract_peaks_cancellable(&flac_fixture(), 64, &cancelled);
        assert!(matches!(result, Err(WaveformError::Cancelled)));
    }

    #[test]
    fn invalid_file_is_a_typed_decode_failure() {
        let directory = tempfile::tempdir().unwrap();
        let invalid = directory.path().join("invalid.flac");
        fs::write(&invalid, b"not audio").unwrap();

        let result = GstreamerWaveformBackend.extract_peaks(&invalid, 64);
        assert!(matches!(result, Err(WaveformError::DecodeFailed(_))));
    }

    #[test]
    fn estimated_mp3_duration_does_not_reject_valid_decoded_samples() {
        let peaks = GstreamerWaveformBackend
            .extract_peaks(&mp3_fixture(), 1_000)
            .unwrap();
        assert_eq!(peaks.len(), 1_000);
        assert!(peaks.iter().any(|peak| *peak > 0));
    }

    #[test]
    fn wav_fixture_produces_one_thousand_peaks() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("fixture.wav");
        let samples = (0..SAMPLE_RATE)
            .map(|index| {
                let phase =
                    std::f64::consts::TAU * 440.0 * f64::from(index) / f64::from(SAMPLE_RATE);
                (phase.sin() * 12_000.0) as i16
            })
            .collect::<Vec<_>>();
        write_wav(&path, &samples);

        let peaks = GstreamerWaveformBackend
            .extract_peaks(&path, 1_000)
            .unwrap();
        assert_eq!(peaks.len(), 1_000);
        assert!(peaks.iter().any(|peak| *peak > 0));
    }

    #[test]
    fn empty_wav_is_a_typed_empty_stream() {
        let directory = tempfile::tempdir().unwrap();
        let empty = directory.path().join("empty.wav");
        write_wav(&empty, &[]);

        let result = GstreamerWaveformBackend.extract_peaks(&empty, 64);
        assert!(matches!(result, Err(WaveformError::EmptyStream)));
    }
}
