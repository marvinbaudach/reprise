//! Native bounded GStreamer decoding for waveform and audio-character work.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use reprise_core::audio_analysis::{
    AnalysisOutput, AudioAnalysisBackend, AudioAnalysisError, AudioEvidenceAccumulator,
    AudioExtractionError,
};
use reprise_core::waveform::STORED_PEAK_COUNT;

const SAMPLE_RATE: u32 = 8_000;
const PIPELINE_DESCRIPTION: &str = "uridecodebin name=decoder ! audioconvert ! audioresample ! \
    audio/x-raw,format=F32LE,channels=1,rate=8000,layout=interleaved ! \
    appsink name=sink sync=false";
const STATE_TIMEOUT: gst::ClockTime = gst::ClockTime::from_seconds(5);
const PULL_TIMEOUT: gst::ClockTime = gst::ClockTime::from_mseconds(50);
const MAX_QUEUED_BUFFERS: u32 = 2;

#[derive(Clone, Copy, Default)]
pub struct GstreamerAudioAnalysisBackend;

impl AudioAnalysisBackend for GstreamerAudioAnalysisBackend {
    fn analyze(
        &self,
        path: &Path,
        cancelled: &AtomicBool,
    ) -> Result<AnalysisOutput, AudioAnalysisError> {
        analyze_native(path, cancelled, STORED_PEAK_COUNT, |_, _| {})
    }
}

pub(crate) fn analyze_for_waveform(
    path: &Path,
    buckets: usize,
    cancelled: &AtomicBool,
) -> Result<Vec<u8>, AudioAnalysisError> {
    analyze_native(path, cancelled, buckets, |_, _| {}).map(|output| output.waveform_peaks)
}

fn analyze_native(
    path: &Path,
    cancelled: &AtomicBool,
    waveform_buckets: usize,
    mut on_chunk: impl FnMut(usize, &AtomicBool),
) -> Result<AnalysisOutput, AudioAnalysisError> {
    if !path.is_file() {
        return Err(AudioAnalysisError::FileNotFound(path.to_path_buf()));
    }
    if cancelled.load(Ordering::Acquire) {
        return Err(AudioAnalysisError::Cancelled);
    }
    gst::init().map_err(|error| AudioAnalysisError::DecodeFailed(error.to_string()))?;
    let (pipeline, sink) = build_pipeline(path)?;
    let result = run_pipeline(&pipeline, &sink, cancelled, waveform_buckets, &mut on_chunk);
    let _ = pipeline.set_state(gst::State::Null);
    result
}

fn build_pipeline(path: &Path) -> Result<(gst::Pipeline, gst_app::AppSink), AudioAnalysisError> {
    let uri = gst::glib::filename_to_uri(path, None)
        .map_err(|_| AudioAnalysisError::DecodeFailed("path cannot be converted to URI".into()))?;
    let pipeline = gst::parse::launch(PIPELINE_DESCRIPTION)
        .map_err(|error| AudioAnalysisError::DecodeFailed(error.to_string()))?
        .downcast::<gst::Pipeline>()
        .map_err(|_| AudioAnalysisError::DecodeFailed("parser did not create a pipeline".into()))?;
    pipeline
        .by_name("decoder")
        .ok_or_else(|| AudioAnalysisError::DecodeFailed("pipeline has no decoder".into()))?
        .set_property("uri", uri.to_string());
    let sink = pipeline
        .by_name("sink")
        .ok_or_else(|| AudioAnalysisError::DecodeFailed("pipeline has no AppSink".into()))?
        .downcast::<gst_app::AppSink>()
        .map_err(|_| AudioAnalysisError::DecodeFailed("sink is not an AppSink".into()))?;
    sink.set_max_buffers(MAX_QUEUED_BUFFERS);
    sink.set_drop(false);
    sink.set_wait_on_eos(false);
    Ok((pipeline, sink))
}

fn run_pipeline(
    pipeline: &gst::Pipeline,
    sink: &gst_app::AppSink,
    cancelled: &AtomicBool,
    waveform_buckets: usize,
    on_chunk: &mut impl FnMut(usize, &AtomicBool),
) -> Result<AnalysisOutput, AudioAnalysisError> {
    pipeline
        .set_state(gst::State::Paused)
        .map_err(|error| AudioAnalysisError::DecodeFailed(error.to_string()))?;
    let (state_result, _, _) = pipeline.state(STATE_TIMEOUT);
    state_result.map_err(|error| AudioAnalysisError::DecodeFailed(error.to_string()))?;
    let duration = pipeline
        .query_duration::<gst::ClockTime>()
        .ok_or_else(|| AudioAnalysisError::DecodeFailed("stream duration is unavailable".into()))?;
    let expected_samples = duration
        .nseconds()
        .saturating_mul(u64::from(SAMPLE_RATE))
        .saturating_add(500_000_000)
        / 1_000_000_000;
    if expected_samples == 0 {
        return Err(AudioAnalysisError::EmptyStream);
    }
    let mut accumulator =
        AudioEvidenceAccumulator::new(SAMPLE_RATE, expected_samples, waveform_buckets)
            .map_err(map_extraction_error)?;
    pipeline
        .set_state(gst::State::Playing)
        .map_err(|error| AudioAnalysisError::DecodeFailed(error.to_string()))?;
    let bus = pipeline
        .bus()
        .ok_or_else(|| AudioAnalysisError::DecodeFailed("pipeline has no bus".into()))?;
    let mut chunks = 0;
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Err(AudioAnalysisError::Cancelled);
        }
        if let Some(sample) = sink.try_pull_sample(PULL_TIMEOUT) {
            push_sample(&mut accumulator, &sample)?;
            chunks += 1;
            on_chunk(chunks, cancelled);
            continue;
        }
        if let Some(message) = bus.timed_pop_filtered(
            gst::ClockTime::ZERO,
            &[gst::MessageType::Eos, gst::MessageType::Error],
        ) {
            match message.view() {
                gst::MessageView::Eos(_) => break,
                gst::MessageView::Error(error) => {
                    return Err(AudioAnalysisError::DecodeFailed(format!(
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
    accumulator.finish().map_err(map_extraction_error)
}

fn push_sample(
    accumulator: &mut AudioEvidenceAccumulator,
    sample: &gst::Sample,
) -> Result<(), AudioAnalysisError> {
    let buffer = sample
        .buffer()
        .ok_or_else(|| AudioAnalysisError::DecodeFailed("sample has no buffer".into()))?;
    let map = buffer
        .map_readable()
        .map_err(|_| AudioAnalysisError::DecodeFailed("sample buffer is unreadable".into()))?;
    let bytes = map.as_slice();
    if !bytes.len().is_multiple_of(size_of::<f32>()) {
        return Err(AudioAnalysisError::DecodeFailed(
            "sample buffer is not aligned F32 audio".into(),
        ));
    }
    let samples = bytes
        .chunks_exact(size_of::<f32>())
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect::<Vec<_>>();
    accumulator.push(&samples).map_err(map_extraction_error)
}

fn map_extraction_error(error: AudioExtractionError) -> AudioAnalysisError {
    match error {
        AudioExtractionError::EmptyAudio => AudioAnalysisError::EmptyStream,
        other => AudioAnalysisError::DecodeFailed(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::{BufWriter, Write};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;
    use std::time::Instant;

    use reprise_core::audio_analysis::{AudioAnalysisBackend, AudioAnalysisError};

    use super::{analyze_native, GstreamerAudioAnalysisBackend, SAMPLE_RATE};

    fn flac_fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sine.flac")
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

    fn write_generated_wav(path: &Path, seconds: u32) {
        let sample_count = SAMPLE_RATE * seconds;
        let data_size = sample_count * u32::try_from(size_of::<i16>()).unwrap();
        let mut file = BufWriter::new(fs::File::create(path).unwrap());
        file.write_all(b"RIFF").unwrap();
        file.write_all(&(36 + data_size).to_le_bytes()).unwrap();
        file.write_all(b"WAVEfmt ").unwrap();
        file.write_all(&16_u32.to_le_bytes()).unwrap();
        file.write_all(&1_u16.to_le_bytes()).unwrap();
        file.write_all(&1_u16.to_le_bytes()).unwrap();
        file.write_all(&SAMPLE_RATE.to_le_bytes()).unwrap();
        file.write_all(&(SAMPLE_RATE * 2).to_le_bytes()).unwrap();
        file.write_all(&2_u16.to_le_bytes()).unwrap();
        file.write_all(&16_u16.to_le_bytes()).unwrap();
        file.write_all(b"data").unwrap();
        file.write_all(&data_size.to_le_bytes()).unwrap();
        for index in 0..sample_count {
            let phase = std::f64::consts::TAU * 440.0 * f64::from(index) / f64::from(SAMPLE_RATE);
            file.write_all(&((phase.sin() * 12_000.0) as i16).to_le_bytes())
                .unwrap();
        }
        file.flush().unwrap();
    }

    fn peak_rss_kib() -> Option<u64> {
        fs::read_to_string("/proc/self/status")
            .ok()?
            .lines()
            .find_map(|line| line.strip_prefix("VmHWM:"))?
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    }

    #[test]
    fn flac_fixture_produces_evidence_and_one_thousand_peaks() {
        let result = GstreamerAudioAnalysisBackend
            .analyze(&flac_fixture(), &AtomicBool::new(false))
            .unwrap();

        assert_eq!(result.waveform_peaks.len(), 1_000);
        assert!(result.waveform_peaks.iter().any(|peak| *peak > 0));
        assert!(result.evidence.loudness_rms() > 0.0);
    }

    #[test]
    fn production_decoder_delivers_multiple_bounded_chunks() {
        let mut observed_chunks = 0;
        analyze_native(&flac_fixture(), &AtomicBool::new(false), 1_000, |_, _| {
            observed_chunks += 1;
        })
        .unwrap();

        assert!(observed_chunks > 1);
    }

    #[test]
    fn wav_fixture_produces_evidence_and_one_thousand_peaks() {
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

        let result = GstreamerAudioAnalysisBackend
            .analyze(&path, &AtomicBool::new(false))
            .unwrap();

        assert_eq!(result.waveform_peaks.len(), 1_000);
        assert!(result.evidence.loudness_rms() > 0.0);
    }

    #[test]
    fn missing_file_is_typed() {
        let missing = Path::new("/tmp/reprise-audio-analysis-missing.flac");
        let result = GstreamerAudioAnalysisBackend.analyze(missing, &AtomicBool::new(false));

        assert!(matches!(result, Err(AudioAnalysisError::FileNotFound(path)) if path == missing));
    }

    #[test]
    fn pre_cancelled_analysis_never_starts() {
        let cancelled = AtomicBool::new(true);
        let result = GstreamerAudioAnalysisBackend.analyze(&flac_fixture(), &cancelled);

        assert!(matches!(result, Err(AudioAnalysisError::Cancelled)));
    }

    #[test]
    fn cancellation_between_chunks_returns_no_partial_result() {
        let cancelled = AtomicBool::new(false);
        let result = analyze_native(&flac_fixture(), &cancelled, 1_000, |chunks, token| {
            if chunks == 1 {
                token.store(true, std::sync::atomic::Ordering::Release);
            }
        });

        assert!(matches!(result, Err(AudioAnalysisError::Cancelled)));
    }

    #[test]
    fn invalid_file_is_a_typed_decode_failure() {
        let directory = tempfile::tempdir().unwrap();
        let invalid = directory.path().join("invalid.flac");
        fs::write(&invalid, b"not audio").unwrap();

        let result = GstreamerAudioAnalysisBackend.analyze(&invalid, &AtomicBool::new(false));

        assert!(matches!(result, Err(AudioAnalysisError::DecodeFailed(_))));
    }

    #[test]
    fn empty_wav_is_a_typed_empty_stream() {
        let directory = tempfile::tempdir().unwrap();
        let empty = directory.path().join("empty.wav");
        write_wav(&empty, &[]);

        let result = GstreamerAudioAnalysisBackend.analyze(&empty, &AtomicBool::new(false));

        assert!(matches!(result, Err(AudioAnalysisError::EmptyStream)));
    }

    #[test]
    #[ignore = "release-profile benchmark; run explicitly and retain same-host results"]
    fn release_profile_short_and_long_fixture_benchmark() {
        const LONG_SECONDS: u32 = 120;

        let short_started = Instant::now();
        let short = GstreamerAudioAnalysisBackend
            .analyze(&flac_fixture(), &AtomicBool::new(false))
            .unwrap();
        let short_elapsed = short_started.elapsed();

        let directory = tempfile::tempdir().unwrap();
        let long_path = directory.path().join("generated-120-seconds.wav");
        write_generated_wav(&long_path, LONG_SECONDS);
        let long_started = Instant::now();
        let long = GstreamerAudioAnalysisBackend
            .analyze(&long_path, &AtomicBool::new(false))
            .unwrap();
        let long_elapsed = long_started.elapsed();

        eprintln!(
            "audio_analysis_benchmark short_ms={} long_audio_seconds={LONG_SECONDS} \
             long_ms={} peak_rss_kib={}",
            short_elapsed.as_millis(),
            long_elapsed.as_millis(),
            peak_rss_kib().map_or_else(|| "unknown".into(), |rss| rss.to_string())
        );
        assert_eq!(short.waveform_peaks.len(), 1_000);
        assert_eq!(long.waveform_peaks.len(), 1_000);
    }
}
