//! GStreamer-backed waveform extraction for Linux frontends.
//!
//! A bounded `uridecodebin` pipeline decodes to calibrated 32 kHz mono F32 PCM
//! and feeds the waveform and — when the caller asked for them — the
//! spectrogram accumulator from that one stream. A peaks-only request skips
//! the bands entirely rather than computing and discarding them. Decoding is
//! memory-bounded (a small queue of buffers) and cancellable between pulled
//! samples.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use lofty::prelude::AudioFile;
use reprise_core::spectrogram::{
    SpectrogramAccumulator, TrackSpectrogram, SPECTROGRAM_SAMPLE_RATE_HZ,
};
use reprise_core::waveform::{
    RenderDataBackend, TrackRenderData, WaveformAccumulator, WaveformBackend, WaveformError,
};

const SAMPLE_RATE: u32 = SPECTROGRAM_SAMPLE_RATE_HZ;
const PIPELINE_DESCRIPTION: &str = "uridecodebin name=decoder ! audioconvert ! audioresample ! \
    audio/x-raw,format=F32LE,channels=1,rate=32000,layout=interleaved ! \
    appsink name=sink sync=false";
const STATE_TIMEOUT: gst::ClockTime = gst::ClockTime::from_seconds(5);
const PULL_TIMEOUT: gst::ClockTime = gst::ClockTime::from_mseconds(50);
const MAX_QUEUED_BUFFERS: u32 = 2;
const NANOSECONDS_PER_SECOND: u64 = 1_000_000_000;
const METADATA_DURATION_HEADROOM_SAMPLES: u64 = SAMPLE_RATE as u64;

#[derive(Clone, Copy, Default)]
pub struct GstreamerWaveformBackend;

/// Which datasets one decode pass is asked to produce. The bands cost real
/// time (two FFTs per stored frame), so a caller that only wants peaks does
/// not pay for them.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RenderRequest {
    PeaksOnly,
    PeaksAndBands,
}

impl RenderDataBackend for GstreamerWaveformBackend {
    fn extract_render_data(
        &self,
        path: &Path,
        buckets: usize,
    ) -> Result<TrackRenderData, WaveformError> {
        extract(
            path,
            buckets,
            &AtomicBool::new(false),
            RenderRequest::PeaksAndBands,
        )
    }

    fn extract_render_data_cancellable(
        &self,
        path: &Path,
        buckets: usize,
        cancelled: &AtomicBool,
    ) -> Result<TrackRenderData, WaveformError> {
        extract(path, buckets, cancelled, RenderRequest::PeaksAndBands)
    }
}

impl WaveformBackend for GstreamerWaveformBackend {
    fn extract_peaks(&self, path: &Path, buckets: usize) -> Result<Vec<u8>, WaveformError> {
        self.extract_peaks_cancellable(path, buckets, &AtomicBool::new(false))
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
        extract(path, buckets, cancelled, RenderRequest::PeaksOnly).map(|data| data.waveform_peaks)
    }
}

fn extract(
    path: &Path,
    buckets: usize,
    cancelled: &AtomicBool,
    request: RenderRequest,
) -> Result<TrackRenderData, WaveformError> {
    if !path.is_file() {
        return Err(WaveformError::FileNotFound(path.to_path_buf()));
    }
    if cancelled.load(Ordering::Acquire) {
        return Err(WaveformError::Cancelled);
    }
    gst::init().map_err(|error| WaveformError::DecodeFailed(error.to_string()))?;
    let (pipeline, sink) = build_pipeline(path)?;
    let result = run_pipeline(path, &pipeline, &sink, cancelled, buckets, request);
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
    path: &Path,
    pipeline: &gst::Pipeline,
    sink: &gst_app::AppSink,
    cancelled: &AtomicBool,
    buckets: usize,
    request: RenderRequest,
) -> Result<TrackRenderData, WaveformError> {
    pipeline
        .set_state(gst::State::Paused)
        .map_err(|error| WaveformError::DecodeFailed(error.to_string()))?;
    let (state_result, _, _) = pipeline.state(STATE_TIMEOUT);
    state_result.map_err(|error| WaveformError::DecodeFailed(error.to_string()))?;
    pipeline
        .set_state(gst::State::Playing)
        .map_err(|error| WaveformError::DecodeFailed(error.to_string()))?;
    let (duration, duration_headroom) = match pipeline.query_duration::<gst::ClockTime>() {
        Some(duration) => (duration, 0),
        None => (
            metadata_duration(path).ok_or_else(|| {
                WaveformError::DecodeFailed("stream duration is unavailable".into())
            })?,
            // Header-only MP3 duration excludes decoder padding on some VBR
            // files. One second is a bounded capacity guard, not a second
            // decode; it shifts a four-minute waveform by under 0.5%.
            METADATA_DURATION_HEADROOM_SAMPLES,
        ),
    };
    let expected_samples = duration
        .nseconds()
        .saturating_mul(u64::from(SAMPLE_RATE))
        // Container duration is commonly fractional after resampling. It is
        // an upper-bound capacity here, not a nearest-sample measurement: a
        // one-sample underestimate would reject an otherwise valid stream.
        .saturating_add(NANOSECONDS_PER_SECOND - 1)
        / NANOSECONDS_PER_SECOND
        + duration_headroom;
    if expected_samples == 0 {
        return Err(WaveformError::EmptyStream);
    }
    let mut waveform = WaveformAccumulator::new(expected_samples, buckets)?;
    let mut spectrogram =
        (request == RenderRequest::PeaksAndBands).then(SpectrogramAccumulator::new);
    let bus = pipeline
        .bus()
        .ok_or_else(|| WaveformError::DecodeFailed("pipeline has no bus".into()))?;
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Err(WaveformError::Cancelled);
        }
        if let Some(sample) = sink.try_pull_sample(PULL_TIMEOUT) {
            push_sample(&mut waveform, spectrogram.as_mut(), &sample)?;
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
    Ok(TrackRenderData {
        waveform_peaks: waveform.finish()?,
        spectrogram: spectrogram
            .map_or_else(TrackSpectrogram::empty, SpectrogramAccumulator::finish),
    })
}

fn metadata_duration(path: &Path) -> Option<gst::ClockTime> {
    let tagged_file = lofty::probe::Probe::open(path).ok()?.read().ok()?;
    let nanoseconds = u64::try_from(tagged_file.properties().duration().as_nanos()).ok()?;
    (nanoseconds > 0).then(|| gst::ClockTime::from_nseconds(nanoseconds))
}

fn push_sample(
    waveform: &mut WaveformAccumulator,
    spectrogram: Option<&mut SpectrogramAccumulator>,
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
    waveform.push(&samples)?;
    if let Some(spectrogram) = spectrogram {
        spectrogram.push(&samples);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;

    use reprise_core::waveform::{RenderDataBackend, WaveformBackend, WaveformError};

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
    fn a_peaks_only_request_computes_no_bands() {
        let peaks_only = extract(
            &flac_fixture(),
            64,
            &AtomicBool::new(false),
            RenderRequest::PeaksOnly,
        )
        .unwrap();
        let both = extract(
            &flac_fixture(),
            64,
            &AtomicBool::new(false),
            RenderRequest::PeaksAndBands,
        )
        .unwrap();

        assert_eq!(peaks_only.waveform_peaks, both.waveform_peaks);
        assert_eq!(
            peaks_only.spectrogram,
            TrackSpectrogram::empty(),
            "a caller that asked for peaks alone must not pay for the bands"
        );
        assert!(both.spectrogram.frame_count() > 0);
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

    #[test]
    fn one_decode_produces_calibrated_peaks_and_one_kilohertz_bands() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("one-kilohertz.wav");
        let samples = (0..SAMPLE_RATE)
            .map(|index| {
                let phase =
                    std::f64::consts::TAU * 1_000.0 * f64::from(index) / f64::from(SAMPLE_RATE);
                (phase.sin() * 0.25 * f64::from(i16::MAX)) as i16
            })
            .collect::<Vec<_>>();
        write_wav(&path, &samples);

        let data = GstreamerWaveformBackend
            .extract_render_data(&path, 1_000)
            .unwrap();
        let final_frame = data
            .spectrogram
            .frame(data.spectrogram.frame_count() - 1)
            .unwrap();

        assert_eq!(data.waveform_peaks.len(), 1_000);
        assert_eq!(data.spectrogram.frame_count(), 20);
        assert_eq!(
            final_frame
                .iter()
                .enumerate()
                .max_by_key(|(_, level)| *level)
                .map(|(index, _)| index),
            Some(14)
        );
        assert!((216..=222).contains(&final_frame[14]));
    }

    #[test]
    #[ignore = "requires two owner-selected real tracks"]
    fn real_tracks_of_different_loudness_keep_visibly_different_levels() {
        let loud_path = std::env::var_os("REPRISE_SPECTROGRAM_LOUD_TRACK")
            .map(PathBuf::from)
            .expect("set REPRISE_SPECTROGRAM_LOUD_TRACK");
        let quiet_path = std::env::var_os("REPRISE_SPECTROGRAM_QUIET_TRACK")
            .map(PathBuf::from)
            .expect("set REPRISE_SPECTROGRAM_QUIET_TRACK");
        let backend = GstreamerWaveformBackend;

        let loud = backend.extract_render_data(&loud_path, 1_000).unwrap();
        let quiet = backend.extract_render_data(&quiet_path, 1_000).unwrap();
        let average_level = |data: &TrackRenderData| {
            data.spectrogram
                .cells()
                .iter()
                .map(|cell| f64::from(*cell))
                .sum::<f64>()
                / data.spectrogram.cells().len() as f64
        };
        let loud_level = average_level(&loud);
        let quiet_level = average_level(&quiet);

        assert!(
            loud_level >= quiet_level + 5.0,
            "expected a visible absolute-level gap: loud={loud_level:.2}, quiet={quiet_level:.2}"
        );
    }
}
