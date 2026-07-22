//! End-to-end evidence run for the real htdemucs backend.
//!
//! `#[ignore]` and gated on a provided model, so the normal gate
//! (`cargo test --all-features`) compiles but never runs it — the suite needs
//! no model and no onnxruntime at runtime. To produce the release-profile
//! realtime-factor evidence the plan asks for:
//!
//! ```bash
//! ORT_DYLIB_PATH=/path/to/libonnxruntime.so.1.22.0 \
//! REPRISE_STEMS_TEST_MODEL=/path/to/htdemucs.onnx \
//!   cargo test -p reprise-stems --features ort --release \
//!   --test e2e_separation -- --ignored --nocapture
//! ```
#![cfg(feature = "ort")]

use std::path::Path;
use std::time::Instant;

use reprise_stems::{OrtStemBackend, StemSeparationBackend, PROGRESS_COMPLETE};

const SAMPLE_RATE: u32 = 44_100;

#[test]
#[ignore = "needs a real htdemucs.onnx via REPRISE_STEMS_TEST_MODEL and onnxruntime via ORT_DYLIB_PATH"]
fn real_htdemucs_separation_realtime_factor() {
    let Ok(model) = std::env::var("REPRISE_STEMS_TEST_MODEL") else {
        eprintln!("SKIP: set REPRISE_STEMS_TEST_MODEL to a local htdemucs.onnx");
        return;
    };
    let model_id =
        std::env::var("REPRISE_STEMS_TEST_MODEL_ID").unwrap_or_else(|_| "htdemucs@4".into());

    // A ~12 s synthetic stereo mix — enough to span several overlapping
    // segments (stride ≈ 5.85 s). Quality is not judged; this is timing +
    // determinism only.
    let seconds = 12usize;
    let frames = seconds * SAMPLE_RATE as usize;
    let (left, right) = synth_stereo(frames);

    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("mix.wav");
    std::fs::write(&source, wav_stereo(&left, &right, SAMPLE_RATE)).unwrap();

    let backend =
        OrtStemBackend::from_model_file(Path::new(&model), &model_id).expect("load htdemucs model");
    assert_eq!(backend.model_id(), model_id);

    // First run: time it and collect progress.
    let out_a = dir.path().join("instr_a.flac");
    let mut progress = Vec::new();
    let started = Instant::now();
    backend
        .separate_instrumental(&source, &out_a, &mut |p| progress.push(p), &|| false)
        .expect("separation succeeds");
    let elapsed = started.elapsed();

    let audio_secs = seconds as f64;
    let wall_secs = elapsed.as_secs_f64();
    let rtf = wall_secs / audio_secs;
    eprintln!("== htdemucs E2E (release profile) ==");
    eprintln!("audio: {audio_secs:.1}s  segments: {}", progress.len());
    eprintln!("wall:  {wall_secs:.2}s");
    eprintln!(
        "realtime factor: {rtf:.3}x  ({:.2} audio-s / wall-s)",
        audio_secs / wall_secs
    );
    eprintln!("peak RSS: {:.0} MiB", peak_rss_mib());

    // Progress contract on the real backend.
    assert!(!progress.is_empty());
    assert!(
        progress.windows(2).all(|w| w[0] <= w[1]),
        "progress monotonic"
    );
    assert_eq!(*progress.last().unwrap(), PROGRESS_COMPLETE);

    // The render is a valid FLAC.
    let bytes_a = std::fs::read(&out_a).unwrap();
    assert_eq!(&bytes_a[0..4], b"fLaC");

    // Determinism: identical input → byte-identical output.
    let out_b = dir.path().join("instr_b.flac");
    backend
        .separate_instrumental(&source, &out_b, &mut |_| {}, &|| false)
        .expect("second separation succeeds");
    let bytes_b = std::fs::read(&out_b).unwrap();
    assert_eq!(
        bytes_a, bytes_b,
        "identical input must yield identical output"
    );
    eprintln!(
        "determinism: OK ({} bytes, identical across two runs)",
        bytes_a.len()
    );
}

#[test]
#[ignore = "needs a real htdemucs.onnx via REPRISE_STEMS_TEST_MODEL and onnxruntime via ORT_DYLIB_PATH"]
fn cancel_stops_a_real_run_without_output() {
    let Ok(model) = std::env::var("REPRISE_STEMS_TEST_MODEL") else {
        eprintln!("SKIP: set REPRISE_STEMS_TEST_MODEL");
        return;
    };
    let frames = 20 * SAMPLE_RATE as usize; // long enough for several segments
    let (left, right) = synth_stereo(frames);
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("mix.wav");
    std::fs::write(&source, wav_stereo(&left, &right, SAMPLE_RATE)).unwrap();

    let backend = OrtStemBackend::from_model_file(Path::new(&model), "htdemucs@4").unwrap();
    let out = dir.path().join("instr.flac");

    // Cancel after the first segment's progress; measure the latency to stop.
    let seen = std::cell::Cell::new(0u32);
    let started = Instant::now();
    let err = backend
        .separate_instrumental(&source, &out, &mut |_| seen.set(seen.get() + 1), &|| {
            seen.get() >= 1
        })
        .unwrap_err();
    eprintln!(
        "cancel latency after 1 segment: {:.2}s",
        started.elapsed().as_secs_f64()
    );
    assert!(matches!(err, reprise_stems::StemError::Cancelled));
    assert!(!out.exists(), "a cancelled run leaves no output");
}

/// Peak resident set size of this process in MiB (Linux `VmHWM`), for the
/// memory-peak evidence the plan asks for. `0.0` if unreadable.
fn peak_rss_mib() -> f64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|line| line.starts_with("VmHWM"))
                .and_then(|line| line.split_whitespace().nth(1))
                .and_then(|kb| kb.parse::<f64>().ok())
        })
        .map_or(0.0, |kb| kb / 1024.0)
}

/// A tone+noise stereo mix — non-trivial spectrum, deterministic.
fn synth_stereo(frames: usize) -> (Vec<f32>, Vec<f32>) {
    let mut left = vec![0f32; frames];
    let mut right = vec![0f32; frames];
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    for i in 0..frames {
        let t = i as f32 / SAMPLE_RATE as f32;
        let tone = 0.3 * (2.0 * std::f32::consts::PI * 220.0 * t).sin()
            + 0.2 * (2.0 * std::f32::consts::PI * 440.0 * t).sin();
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let noise = ((seed >> 40) as f32 / 8_388_608.0 - 1.0) * 0.05;
        left[i] = tone + noise;
        right[i] = 0.9 * tone - noise;
    }
    (left, right)
}

/// Minimal PCM16 stereo WAV.
fn wav_stereo(left: &[f32], right: &[f32], rate: u32) -> Vec<u8> {
    let frames = left.len();
    let data_len = (frames * 2 * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    let to_i16 = |x: f32| (x.clamp(-1.0, 1.0) * 32_767.0) as i16;
    for (l, r) in left.iter().zip(right) {
        out.extend_from_slice(&to_i16(*l).to_le_bytes());
        out.extend_from_slice(&to_i16(*r).to_le_bytes());
    }
    out
}
