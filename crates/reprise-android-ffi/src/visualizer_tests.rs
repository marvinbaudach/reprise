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
fn the_encoded_scene_is_little_endian_float_bytes() {
    let scene = Scene {
        shapes: vec![
            Shape {
                geom: Geom::Rect {
                    x: 1.0,
                    y: 2.0,
                    w: 3.0,
                    h: 4.0,
                },
                fill: Fill::Solid(Rgba {
                    r: 1.0,
                    g: 0.5,
                    b: 0.25,
                    a: 1.0,
                }),
                width: 2.0,
                glow: 0.5,
                dash: None,
            },
            Shape {
                geom: Geom::RadialGlow {
                    cx: 5.0,
                    cy: 6.0,
                    r: 7.0,
                },
                fill: Fill::Solid(Rgba {
                    r: 0.0,
                    g: 0.25,
                    b: 1.0,
                    a: 0.5,
                }),
                width: 0.0,
                glow: 0.25,
                dash: None,
            },
        ],
    };
    let expected = vec![
        0x00, 0x00, 0x00, 0x00, // rectangle kind
        0x00, 0x00, 0x80, 0x3f, // red = 1.0
        0x00, 0x00, 0x00, 0x3f, // green = 0.5
        0x00, 0x00, 0x80, 0x3e, // blue = 0.25
        0x00, 0x00, 0x80, 0x3f, // alpha = 1.0
        0x00, 0x00, 0x00, 0x40, // width = 2.0
        0x00, 0x00, 0x00, 0x3f, // glow = 0.5
        0x00, 0x00, 0x80, 0x40, // four geometry scalars
        0x00, 0x00, 0x80, 0x3f, // x = 1.0
        0x00, 0x00, 0x00, 0x40, // y = 2.0
        0x00, 0x00, 0x40, 0x40, // width = 3.0
        0x00, 0x00, 0x80, 0x40, // height = 4.0
        0x00, 0x00, 0x00, 0x40, // radial-glow kind
        0x00, 0x00, 0x00, 0x00, // red = 0.0
        0x00, 0x00, 0x80, 0x3e, // green = 0.25
        0x00, 0x00, 0x80, 0x3f, // blue = 1.0
        0x00, 0x00, 0x00, 0x3f, // alpha = 0.5
        0x00, 0x00, 0x00, 0x00, // width = 0.0
        0x00, 0x00, 0x80, 0x3e, // glow = 0.25
        0x00, 0x00, 0x40, 0x40, // three geometry scalars
        0x00, 0x00, 0xa0, 0x40, // cx = 5.0
        0x00, 0x00, 0xc0, 0x40, // cy = 6.0
        0x00, 0x00, 0xe0, 0x40, // radius = 7.0
    ];

    assert_eq!(encode_scene(&scene), expected);
}

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
fn stream_generation_reset_starts_idle_fade_and_phase_at_zero_after_a_time_gap() {
    let delayed_clock = Arc::new(FakeMonotonicClock::default());
    let delayed = AndroidVisualEngine::with_clock(delayed_clock.clone());
    delayed.set_playing(true);
    let pcm = stereo_sine_pcm16(80.0, 48_000, 0, 8_192);
    assert!(delayed.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2));

    delayed_clock.advance(Duration::from_secs(2));
    delayed.reset_audio_stream();
    delayed.tick();

    let immediate_clock = Arc::new(FakeMonotonicClock::default());
    let immediate = AndroidVisualEngine::with_clock(immediate_clock);
    immediate.set_playing(true);
    assert!(immediate.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2));
    immediate.reset_audio_stream();
    immediate.tick();

    assert_eq!(
        delayed.scene(272.0, 272.0),
        immediate.scene(272.0, 272.0),
        "pre-reset playing time advanced the idle fade or phase"
    );
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
fn paused_scene_uses_elapsed_time_at_a_fifteen_hertz_redraw_rate() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.set_playing(true);
    let pcm = stereo_sine_pcm16(80.0, 48_000, 0, 8_192);
    assert!(engine.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2));
    engine.set_playing(false);
    clock.advance(Duration::from_secs(2));
    engine.tick();
    let start = engine.scene(272.0, 272.0);

    // Three 15 Hz frames span exactly 200 ms despite millisecond rounding.
    // Ninety redraws therefore cover the portable wave's six-second period.
    for elapsed in [67, 67, 66].into_iter().cycle().take(90) {
        clock.advance(Duration::from_millis(elapsed));
        engine.tick();
    }
    let after_one_period = engine.scene(272.0, 272.0);

    assert_eq!(start.len(), after_one_period.len());
    let start = decode_float_bytes(&start);
    let after_one_period = decode_float_bytes(&after_one_period);
    let largest_error = start
        .iter()
        .zip(after_one_period)
        .map(|(before, after)| (before - after).abs())
        .fold(0.0_f32, f32::max);
    assert!(
        largest_error < 0.000_1,
        "paused Android scene missed its six-second return by {largest_error}"
    );
}

#[test]
fn paused_stored_analysis_keeps_the_existing_generic_fallback() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let stored = AndroidVisualEngine::with_clock(clock.clone());
    stored.ingest_bands(
        (0..64)
            .map(|index| 0.2 + index as f32 * 0.7 / 63.0)
            .collect(),
    );
    let generic = AndroidVisualEngine::with_clock(clock.clone());
    generic.ingest_bands(Vec::new());

    for _ in 0..60 {
        clock.advance(Duration::from_nanos(16_666_667));
        stored.tick();
        generic.tick();
    }

    let stored_scene = stored.scene(272.0, 272.0);
    let generic_scene = generic.scene(272.0, 272.0);
    assert_eq!(stored_scene.len(), generic_scene.len());
    let stored_scene = decode_float_bytes(&stored_scene);
    let generic_scene = decode_float_bytes(&generic_scene);
    let largest_error = stored_scene
        .iter()
        .zip(&generic_scene)
        .map(|(stored, generic)| (stored - generic).abs())
        .fold(0.0, f32::max);
    assert!(
        largest_error < 0.000_02,
        "stored analysis replaced the generic paused fallback by {largest_error}"
    );
}

#[test]
fn live_pcm_staleness_pauses_while_playback_is_not_intended() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.set_playback_intended(true);
    engine.set_playing(true);
    let pcm = stereo_sine_pcm16(80.0, 48_000, 0, 8_192);
    assert!(engine.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2));

    engine.set_playback_intended(false);
    engine.set_playing(false);
    clock.advance(LIVE_AUDIO_STALE_AFTER + LIVE_AUDIO_STALE_AFTER);
    assert!(engine.has_live_audio());
}

#[test]
fn live_pcm_staleness_expires_while_player_buffers_with_playback_intent() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.set_playback_intended(true);
    engine.set_playing(true);
    let pcm = stereo_sine_pcm16(80.0, 48_000, 0, 8_192);
    assert!(engine.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2));

    // The engine has no Buffering state of its own. This isolates the generic
    // 500 ms expiry while playback intent and visual evolution remain active;
    // Kotlin separately owns the Buffering-to-active projection.
    assert!(engine.bass_pressure().pressure > 0.0);
    clock.advance(LIVE_AUDIO_STALE_AFTER);

    assert!(!engine.has_live_audio());
    assert_eq!(engine.bass_pressure().pressure, 0.0);
}

#[test]
fn live_pcm_staleness_expires_when_playback_intent_was_never_reported() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.set_playing(true);
    let pcm = stereo_sine_pcm16(80.0, 48_000, 0, 8_192);
    assert!(engine.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2));

    clock.advance(LIVE_AUDIO_STALE_AFTER);

    assert!(!engine.has_live_audio());
}

#[test]
fn resume_history_reset_preserves_live_scene_until_pcm_restarts_or_expires() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.set_playback_intended(true);
    engine.set_playing(true);
    let pcm = stereo_sine_pcm16(80.0, 48_000, 0, 8_192);
    assert!(engine.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2));

    engine.set_playback_intended(false);
    engine.set_playing(false);
    let paused_scene = engine.scene(272.0, 272.0);
    clock.advance(LIVE_AUDIO_STALE_AFTER + LIVE_AUDIO_STALE_AFTER);

    // Media3 publishes playWhenReady before isPlaying on a real resume.
    engine.set_playback_intended(true);
    engine.reset_audio_history();
    engine.set_playing(true);

    assert!(engine.has_live_audio());
    assert_eq!(engine.scene(272.0, 272.0), paused_scene);
    assert!(engine
        .live_bands_for_testing()
        .iter()
        .all(|band| *band == 0.0));

    clock.advance(LIVE_AUDIO_STALE_AFTER);
    assert!(!engine.has_live_audio());
}

#[test]
fn stale_live_pcm_reopens_the_stored_spectrogram_fallback() {
    let clock = Arc::new(FakeMonotonicClock::default());
    let engine = AndroidVisualEngine::with_clock(clock.clone());
    engine.set_playback_intended(true);
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

#[test]
fn a_band_frame_dropped_on_contention_is_counted() {
    let engine = AndroidVisualEngine::new();
    let pcm = stereo_sine_pcm16(80.0, 48_000, 0, 512);

    assert_eq!(engine.dropped_audio_frames(), 0);
    let accepted = engine.with_state_locked_for_testing(|| {
        engine.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2)
    });

    assert!(!accepted);
    assert_eq!(engine.dropped_audio_frames(), 1);
    assert!(engine.ingest_pcm_i16(pcm.clone(), pcm.len() as u32, 48_000, 2));
    assert_eq!(engine.dropped_audio_frames(), 1);
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

fn decode_float_bytes(buffer: &[u8]) -> Vec<f32> {
    assert!(buffer.len().is_multiple_of(size_of::<f32>()));
    let (encoded, rest) = buffer.as_chunks::<{ size_of::<f32>() }>();
    assert!(rest.is_empty(), "the assertion above rules this out");
    encoded.iter().copied().map(f32::from_le_bytes).collect()
}

fn decode_scene(buffer: &[u8]) -> Scene {
    let buffer = decode_float_bytes(buffer);
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
