use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use reprise_core::visuals::{Fill, Geom, Rgba, Scene, Shape};

use crate::visualizer::{
    encode_scene, AndroidVisualEngine, MonotonicClock, LIVE_AUDIO_STALE_AFTER,
};

#[test]
fn flat_scene_layout_round_trips_every_supported_geometry() {
    let scene = Scene {
        shapes: vec![
            shape(Geom::Rect {
                x: 1.0,
                y: 2.0,
                w: 3.0,
                h: 4.0,
            }),
            shape(Geom::Polyline {
                points: vec![(5.0, 6.0), (7.0, 8.0)],
                closed: false,
            }),
            shape(Geom::RadialGlow {
                cx: 9.0,
                cy: 10.0,
                r: 11.0,
            }),
        ],
    };

    let decoded = decode_scene(&encode_scene(&scene));

    assert_eq!(decoded.shapes.len(), scene.shapes.len());
    for (actual, expected) in decoded.shapes.iter().zip(&scene.shapes) {
        assert_shape_eq(actual, expected);
    }
}

#[test]
fn bars_scene_buffer_round_trips_to_finite_sane_shapes_at_phone_and_desktop_sizes() {
    let engine = AndroidVisualEngine::new();
    engine.set_playing(true);
    engine.set_accent(0.2, 0.7, 0.7);
    engine.ingest_bands(vec![0.72; 24]);

    for (width, height) in [
        (64.0, 64.0),
        (272.0, 272.0),
        (548.0, 300.0),
        (4096.0, 256.0),
    ] {
        let decoded = decode_scene(&engine.scene(width, height));
        assert!(decoded.is_finite_and_sane(width, height));
        let kinds = decoded
            .shapes
            .iter()
            .map(|shape| match shape.geom {
                Geom::Rect { .. } => 0,
                Geom::Polyline { .. } => 1,
                Geom::RadialGlow { .. } => 2,
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(kinds, BTreeSet::from([0, 2]));
    }
}

#[test]
fn scene_before_the_first_ingest_is_empty() {
    let engine = AndroidVisualEngine::new();
    engine.set_playing(true);
    engine.note_track_changed();

    assert!(engine.scene(272.0, 272.0).is_empty());
}

#[test]
fn an_empty_analysis_uses_the_shared_resting_scene_while_playback_runs() {
    let engine = AndroidVisualEngine::new();
    engine.set_playing(true);
    engine.ingest_bands(Vec::new());
    for _ in 0..25 {
        engine.tick();
    }

    let scene = decode_scene(&engine.scene(272.0, 272.0));

    assert!(
        scene
            .shapes
            .iter()
            .any(|shape| matches!(shape.geom, Geom::Rect { .. })),
        "the no-analysis scene should contain the engine's resting bars"
    );
}

#[test]
fn live_pcm_produces_sixty_four_non_interpolated_cava_bands() {
    let engine = AndroidVisualEngine::new();
    engine.set_playing(true);

    for chunk in 0..20 {
        let pcm = stereo_sine_pcm16(2_000.0, 48_000, chunk, 512);
        assert!(engine.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2));
    }

    let bands = engine.live_bands_for_testing();
    let largest_neighbor_step = bands
        .windows(2)
        .map(|pair| (pair[0] - pair[1]).abs())
        .fold(0.0_f32, f32::max);
    assert_eq!(bands.len(), 64);
    assert!(
        largest_neighbor_step > 1.0 / 23.0,
        "direct CAVA bins should retain detail finer than one 24-band interpolation step: {bands:?}"
    );
    assert!(engine.has_live_audio());
    assert!(!engine.scene(272.0, 272.0).is_empty());
}

#[test]
fn stereo_pcm_is_averaged_to_mono_instead_of_summed() {
    let engine = AndroidVisualEngine::new();
    let pcm = opposite_phase_stereo_pcm16(512);

    assert!(engine.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2));

    assert!(engine
        .live_bands_for_testing()
        .iter()
        .all(|band| *band == 0.0));
    assert_eq!(engine.bass_pressure().pressure, 0.0);
}

#[test]
fn stream_and_track_changes_reset_cava_and_bass_history() {
    let engine = AndroidVisualEngine::new();
    let pcm = stereo_sine_pcm16(80.0, 48_000, 0, 8_192);
    assert!(engine.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2));
    assert!(engine
        .live_bands_for_testing()
        .iter()
        .any(|band| *band > 0.0));

    engine.reset_audio_stream();
    assert!(!engine.has_live_audio());
    assert!(engine
        .live_bands_for_testing()
        .iter()
        .all(|band| *band == 0.0));
    assert_eq!(engine.bass_pressure().pressure, 0.0);

    assert!(engine.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2));
    engine.note_track_changed();
    assert!(!engine.has_live_audio());
    assert!(engine
        .live_bands_for_testing()
        .iter()
        .all(|band| *band == 0.0));
    assert_eq!(engine.bass_pressure().pressure, 0.0);
}

#[test]
fn paused_live_audio_reports_silence_without_forgetting_the_stream() {
    let engine = AndroidVisualEngine::new();
    engine.set_playing(true);
    let pcm = stereo_sine_pcm16(80.0, 48_000, 0, 8_192);
    assert!(engine.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2));

    engine.set_playing(false);

    assert!(engine.has_live_audio());
    assert_eq!(engine.bass_pressure().kick, 0.0);
    assert_eq!(engine.bass_pressure().pressure, 0.0);
}

#[test]
fn stale_live_pcm_reopens_the_stored_spectrogram_fallback() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.set_playing(true);
    let pcm = stereo_sine_pcm16(80.0, 48_000, 0, 8_192);
    assert!(engine.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2));
    assert!(engine.has_live_audio());

    clock.advance(LIVE_AUDIO_STALE_AFTER);
    assert!(!engine.has_live_audio());

    let stored_bands = vec![0.35; 24];
    engine.ingest_bands(stored_bands.clone());
    let fallback = AndroidVisualEngine::with_clock(clock);
    fallback.set_playing(true);
    fallback.ingest_bands(stored_bands);

    let stale_scene = decode_scene(&engine.scene(272.0, 272.0));
    let fallback_scene = decode_scene(&fallback.scene(272.0, 272.0));
    assert_eq!(
        main_bar_segments(&stale_scene, 272.0),
        main_bar_segments(&fallback_scene, 272.0),
    );
}

#[test]
fn ui_reads_do_not_wait_for_live_pcm_processing() {
    let engine = Arc::new(AndroidVisualEngine::new());

    let (read, worker) = engine.with_live_processor_locked_for_testing(|| {
        let (sender, receiver) = mpsc::channel();
        let engine = Arc::clone(&engine);
        let worker = thread::spawn(move || {
            sender
                .send(engine.has_live_audio())
                .expect("test receiver remains alive");
        });
        (receiver.recv_timeout(Duration::from_millis(250)), worker)
    });

    worker.join().expect("UI read worker should finish");
    assert!(!read.expect("UI read waited for the PCM processor"));
}

#[derive(Default)]
struct FakeMonotonicClock {
    now_nanos: AtomicU64,
}

impl FakeMonotonicClock {
    fn advance(&self, duration: Duration) {
        self.now_nanos.fetch_add(
            duration
                .as_nanos()
                .try_into()
                .expect("test duration fits u64"),
            Ordering::Relaxed,
        );
    }
}

impl MonotonicClock for FakeMonotonicClock {
    fn now(&self) -> Duration {
        Duration::from_nanos(self.now_nanos.load(Ordering::Relaxed))
    }
}

fn main_bar_segments(scene: &Scene, height: f32) -> Vec<[f32; 4]> {
    scene
        .shapes
        .iter()
        .filter_map(|shape| match shape.geom {
            Geom::Rect { x, y, w, h } if y < height * 0.82 && h > 3.0 => Some([x, y, w, h]),
            _ => None,
        })
        .collect()
}

fn shape(geom: Geom) -> Shape {
    Shape {
        geom,
        fill: Fill::Solid(Rgba {
            r: 0.1,
            g: 0.2,
            b: 0.3,
            a: 0.4,
        }),
        width: 1.5,
        glow: 0.6,
        dash: None,
    }
}

fn stereo_sine_pcm16(
    frequency_hz: f32,
    sample_rate_hz: u32,
    chunk: usize,
    frame_count: usize,
) -> Vec<u8> {
    let mut pcm = Vec::with_capacity(frame_count * 4);
    for frame in 0..frame_count {
        let absolute_frame = chunk * frame_count + frame;
        let sample = (std::f32::consts::TAU * frequency_hz * absolute_frame as f32
            / sample_rate_hz as f32)
            .sin();
        let sample = (sample * 20_000.0).round() as i16;
        pcm.extend_from_slice(&sample.to_le_bytes());
        pcm.extend_from_slice(&sample.to_le_bytes());
    }
    pcm
}

fn opposite_phase_stereo_pcm16(frame_count: usize) -> Vec<u8> {
    let mut pcm = Vec::with_capacity(frame_count * 4);
    for frame in 0..frame_count {
        let left = ((frame as i32 * 997 % 40_000) - 20_000) as i16;
        pcm.extend_from_slice(&left.to_le_bytes());
        pcm.extend_from_slice(&left.saturating_neg().to_le_bytes());
    }
    pcm
}

fn decode_scene(buffer: &[f32]) -> Scene {
    let mut cursor = 0;
    let mut shapes = Vec::new();
    while cursor < buffer.len() {
        let kind = buffer[cursor] as u8;
        let fill = Fill::Solid(Rgba {
            r: buffer[cursor + 1],
            g: buffer[cursor + 2],
            b: buffer[cursor + 3],
            a: buffer[cursor + 4],
        });
        let width = buffer[cursor + 5];
        let glow = buffer[cursor + 6];
        let count = buffer[cursor + 7] as usize;
        cursor += 8;
        let geom = match kind {
            0 => {
                assert_eq!(count, 4);
                let geom = Geom::Rect {
                    x: buffer[cursor],
                    y: buffer[cursor + 1],
                    w: buffer[cursor + 2],
                    h: buffer[cursor + 3],
                };
                cursor += 4;
                geom
            }
            1 => {
                let points = (0..count)
                    .map(|point| (buffer[cursor + point * 2], buffer[cursor + point * 2 + 1]))
                    .collect();
                cursor += count * 2;
                Geom::Polyline {
                    points,
                    closed: false,
                }
            }
            2 => {
                assert_eq!(count, 3);
                let geom = Geom::RadialGlow {
                    cx: buffer[cursor],
                    cy: buffer[cursor + 1],
                    r: buffer[cursor + 2],
                };
                cursor += 3;
                geom
            }
            other => panic!("unknown flat scene kind {other}"),
        };
        shapes.push(Shape {
            geom,
            fill,
            width,
            glow,
            dash: None,
        });
    }
    Scene { shapes }
}

fn assert_shape_eq(actual: &Shape, expected: &Shape) {
    assert_eq!(actual.width, expected.width);
    assert_eq!(actual.glow, expected.glow);
    match (&actual.fill, &expected.fill) {
        (Fill::Solid(actual), Fill::Solid(expected)) => {
            assert_eq!(actual.r, expected.r);
            assert_eq!(actual.g, expected.g);
            assert_eq!(actual.b, expected.b);
            assert_eq!(actual.a, expected.a);
        }
    }
    match (&actual.geom, &expected.geom) {
        (
            Geom::Rect {
                x: ax,
                y: ay,
                w: aw,
                h: ah,
            },
            Geom::Rect {
                x: ex,
                y: ey,
                w: ew,
                h: eh,
            },
        ) => assert_eq!([ax, ay, aw, ah], [ex, ey, ew, eh]),
        (
            Geom::Polyline { points: actual, .. },
            Geom::Polyline {
                points: expected, ..
            },
        ) => assert_eq!(actual, expected),
        (
            Geom::RadialGlow {
                cx: ax,
                cy: ay,
                r: ar,
            },
            Geom::RadialGlow {
                cx: ex,
                cy: ey,
                r: er,
            },
        ) => assert_eq!([ax, ay, ar], [ex, ey, er]),
        _ => panic!("geometry changed kind across the flat scene boundary"),
    }
}
