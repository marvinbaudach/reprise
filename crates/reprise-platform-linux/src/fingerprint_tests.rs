use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use gst::prelude::*;
use gstreamer as gst;
use reprise_core::fingerprint::{
    FingerprintBackend, FingerprintCapability, FingerprintControl, FingerprintError,
    FingerprintOutcome,
};
use tempfile::TempDir;

use crate::fingerprint::{
    cache_namespace, capability_with, pipeline_description, GstreamerFingerprintBackend,
    REQUIRED_ELEMENTS,
};

const SAMPLE_RATE: u32 = 11_025;
const FIXTURE_SECONDS: u32 = 13;

fn write_u16(writer: &mut impl Write, value: u16) {
    writer.write_all(&value.to_le_bytes()).unwrap();
}

fn write_u32(writer: &mut impl Write, value: u32) {
    writer.write_all(&value.to_le_bytes()).unwrap();
}

fn generated_fixture(directory: &TempDir) -> PathBuf {
    generated_fixture_with_seconds(directory, FIXTURE_SECONDS)
}

fn generated_fixture_with_seconds(directory: &TempDir, seconds: u32) -> PathBuf {
    generated_fixture_with_tail(directory, seconds, 659.25)
}

fn generated_fixture_with_tail(directory: &TempDir, seconds: u32, tail_frequency: f64) -> PathBuf {
    let path = directory
        .path()
        .join(format!("fingerprint-chirp-{seconds}-{tail_frequency}.wav"));
    let samples = SAMPLE_RATE * seconds;
    let data_bytes = samples * 2;
    let mut writer = BufWriter::new(File::create(&path).unwrap());
    writer.write_all(b"RIFF").unwrap();
    write_u32(&mut writer, 36 + data_bytes);
    writer.write_all(b"WAVEfmt ").unwrap();
    write_u32(&mut writer, 16);
    write_u16(&mut writer, 1);
    write_u16(&mut writer, 1);
    write_u32(&mut writer, SAMPLE_RATE);
    write_u32(&mut writer, SAMPLE_RATE * 2);
    write_u16(&mut writer, 2);
    write_u16(&mut writer, 16);
    writer.write_all(b"data").unwrap();
    write_u32(&mut writer, data_bytes);
    for index in 0..samples {
        let time = f64::from(index) / f64::from(SAMPLE_RATE);
        let frequency = if index >= SAMPLE_RATE * 120 {
            tail_frequency
        } else if index % (SAMPLE_RATE * 2) < SAMPLE_RATE {
            220.0 + time * 17.0
        } else {
            659.25
        };
        let sample = (f64::sin(time * frequency * std::f64::consts::TAU) * 18_000.0) as i16;
        writer.write_all(&sample.to_le_bytes()).unwrap();
    }
    writer.flush().unwrap();
    path
}

fn completed(outcome: FingerprintOutcome) -> reprise_core::fingerprint::Fingerprint {
    match outcome {
        FingerprintOutcome::Completed(fingerprint) => fingerprint,
        FingerprintOutcome::Cancelled => panic!("expected a completed fingerprint"),
    }
}

#[test]
fn capability_probe_reports_every_missing_required_factory() {
    let capability = capability_with(
        || Ok(()),
        |element| !matches!(element, "capsfilter" | "chromaprint" | "audiobuffersplit"),
        || None,
    );
    assert_eq!(
        capability,
        FingerprintCapability::MissingPlugin {
            elements: vec![
                "capsfilter".into(),
                "audiobuffersplit".into(),
                "chromaprint".into()
            ]
        }
    );
}

#[test]
fn capability_probe_distinguishes_backend_initialization_failure() {
    let capability = capability_with(
        || Err("registry unavailable".into()),
        |_| true,
        || Some("unused".into()),
    );
    assert_eq!(
        capability,
        FingerprintCapability::BackendInitFailed {
            detail: "registry unavailable".into()
        }
    );
}

#[test]
fn capability_namespace_contains_the_injected_plugin_version_and_pipeline_revision() {
    let capability = capability_with(|| Ok(()), |_| true, || Some("1.28.4/custom build".into()));
    assert_eq!(
        capability,
        FingerprintCapability::Available {
            cache_namespace: "gst-chromaprint-plugin-1.28.4-custom-build-pipeline-v1".into()
        }
    );
    assert_eq!(
        cache_namespace("1.28.4"),
        "gst-chromaprint-plugin-1.28.4-pipeline-v1"
    );
}

#[test]
fn pipeline_is_bounded_to_the_first_120_seconds() {
    let description = pipeline_description(0);
    assert!(description.contains("output-buffer-duration=1/1"));
    assert!(description.contains("eos-after=121"));
    assert!(description.contains("duration=120"));
    assert!(description.contains("format=S16LE,channels=1,rate=11025"));
    assert_eq!(REQUIRED_ELEMENTS.len(), 8);
    assert!(REQUIRED_ELEMENTS.contains(&"capsfilter"));
}

#[test]
fn backend_reports_host_capability_and_stable_namespace() {
    if let FingerprintCapability::Available { cache_namespace } =
        GstreamerFingerprintBackend.capability()
    {
        assert!(cache_namespace.starts_with("gst-chromaprint-plugin-"));
        assert!(cache_namespace.ends_with("-pipeline-v1"));
    }
}

#[test]
fn generated_audio_produces_a_deterministic_nonempty_fingerprint_and_full_duration() {
    let directory = TempDir::new().unwrap();
    let path = generated_fixture(&directory);
    let backend = GstreamerFingerprintBackend;
    let FingerprintCapability::Available { cache_namespace } = backend.capability() else {
        return;
    };
    let first = completed(
        backend
            .fingerprint(&path, &mut |_| FingerprintControl::Continue)
            .unwrap(),
    );
    let second = completed(
        backend
            .fingerprint(&path, &mut |_| FingerprintControl::Continue)
            .unwrap(),
    );

    assert!(!first.encoded.is_empty());
    assert_eq!(first, second);
    assert_eq!(first.duration_seconds, u64::from(FIXTURE_SECONDS));
    assert_eq!(first.cache_namespace, cache_namespace);
}

#[test]
fn capped_fingerprint_ignores_different_tails_and_reports_each_full_duration() {
    const FIRST_SECONDS: u32 = 121;
    const SECOND_SECONDS: u32 = 122;
    let directory = TempDir::new().unwrap();
    if !matches!(
        GstreamerFingerprintBackend.capability(),
        FingerprintCapability::Available { .. }
    ) {
        return;
    }
    let first_path = generated_fixture_with_tail(&directory, FIRST_SECONDS, 330.0);
    let second_path = generated_fixture_with_tail(&directory, SECOND_SECONDS, 880.0);
    let first = completed(
        GstreamerFingerprintBackend
            .fingerprint(&first_path, &mut |_| FingerprintControl::Continue)
            .unwrap(),
    );
    let second = completed(
        GstreamerFingerprintBackend
            .fingerprint(&second_path, &mut |_| FingerprintControl::Continue)
            .unwrap(),
    );

    assert_eq!(first.encoded, second.encoded);
    assert_eq!(first.duration_seconds, u64::from(FIRST_SECONDS));
    assert_eq!(second.duration_seconds, u64::from(SECOND_SECONDS));
}

#[test]
fn cancellation_before_decode_is_a_typed_successful_outcome() {
    let directory = TempDir::new().unwrap();
    let path = generated_fixture(&directory);
    let outcome = GstreamerFingerprintBackend
        .fingerprint(&path, &mut |_| FingerprintControl::Cancel)
        .unwrap();
    assert_eq!(outcome, FingerprintOutcome::Cancelled);
}

#[test]
fn cancellation_during_decode_stops_cooperatively() {
    let directory = TempDir::new().unwrap();
    let path = generated_fixture(&directory);
    if !matches!(
        GstreamerFingerprintBackend.capability(),
        FingerprintCapability::Available { .. }
    ) {
        return;
    }
    let started = AtomicBool::new(false);
    let outcome = GstreamerFingerprintBackend
        .fingerprint_with_identity_sleep(&path, 20_000, &mut |progress| {
            if progress.processed_seconds > 0 {
                started.store(true, Ordering::SeqCst);
                FingerprintControl::Cancel
            } else {
                FingerprintControl::Continue
            }
        })
        .unwrap();
    assert!(started.load(Ordering::SeqCst));
    assert_eq!(outcome, FingerprintOutcome::Cancelled);
}

#[test]
fn missing_files_and_per_file_decode_failures_are_typed() {
    let directory = TempDir::new().unwrap();
    let missing_path = directory.path().join("missing.wav");
    let missing = GstreamerFingerprintBackend
        .fingerprint(&missing_path, &mut |_| FingerprintControl::Continue);
    assert!(matches!(missing, Err(FingerprintError::FileNotFound(_))));

    if !matches!(
        GstreamerFingerprintBackend.capability(),
        FingerprintCapability::Available { .. }
    ) {
        return;
    }

    let invalid = directory.path().join("invalid.wav");
    std::fs::write(&invalid, b"not audio").unwrap();
    let failure =
        GstreamerFingerprintBackend.fingerprint(&invalid, &mut |_| FingerprintControl::Continue);
    assert!(matches!(failure, Err(FingerprintError::DecodeFailed(_))));
}

#[test]
fn pipeline_cleanup_always_returns_the_pipeline_to_null() {
    if gst::init().is_err()
        || gst::ElementFactory::find("fakesrc").is_none()
        || gst::ElementFactory::find("fakesink").is_none()
    {
        return;
    }
    let pipeline = gst::parse::launch("fakesrc ! fakesink")
        .unwrap()
        .downcast::<gst::Pipeline>()
        .unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();
    crate::fingerprint::set_null_on_drop_for_test(pipeline.clone());
    assert_eq!(pipeline.current_state(), gst::State::Null);
}
