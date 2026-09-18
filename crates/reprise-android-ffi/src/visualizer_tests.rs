use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use reprise_core::playback::SPECTRUM_BAND_COUNT;
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

#[path = "visualizer_stream_reset_tests.rs"]
mod stream_reset_tests;

#[path = "visualizer_staleness_tests.rs"]
mod staleness_tests;

#[path = "visualizer_shape_adoption_tests.rs"]
mod shape_adoption_tests;

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

fn ingest_one_live_block(
    engine: &AndroidVisualEngine,
    clock: &FakeMonotonicClock,
    pcm: &[u8],
    sample_rate_hz: u32,
) {
    assert!(engine.ingest_pcm_i16(pcm.to_vec(), pcm.len() as u32, sample_rate_hz, 2));
    let frame_count = pcm.len() / (2 * size_of::<i16>());
    clock.advance(Duration::from_secs_f64(
        frame_count as f64 / f64::from(sample_rate_hz),
    ));
    assert!(engine.tick());
}

fn ingest_one_live_mono_block(
    engine: &AndroidVisualEngine,
    clock: &FakeMonotonicClock,
    pcm: &[u8],
    sample_rate_hz: u32,
) {
    assert!(engine.ingest_pcm_i16(pcm.to_vec(), pcm.len() as u32, sample_rate_hz, 1));
    let frame_count = pcm.len() / size_of::<i16>();
    clock.advance(Duration::from_secs_f64(
        frame_count as f64 / f64::from(sample_rate_hz),
    ));
    assert!(engine.tick());
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
