use std::collections::BTreeSet;

use reprise_core::visuals::{Fill, Geom, Rgba, Scene, Shape};

use crate::visualizer::{encode_scene, AndroidVisualEngine};

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
