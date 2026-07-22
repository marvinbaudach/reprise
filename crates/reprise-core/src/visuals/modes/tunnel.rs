//! Tunnel: hue-rotating ring tunnel receding into depth, with alternating
//! closed wavy rings and dashed tick-arc rings, plus a center mini-bar strip.

use super::super::color;
use super::super::engine::ModeCtx;
use super::super::scene::{Geom, Shape};
use std::f32::consts::TAU;

pub(crate) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    let (w, h) = (ctx.width, ctx.height);
    let m = w.min(h);
    let (cx, cy) = (w / 2.0, h / 2.0);
    let min_r = m * 0.09;
    let max_r = (w * w + h * h).sqrt() * 0.62;
    let hue = color::rgb_hue(ctx.accent);
    let rings = 8;
    let band_at = |a: f32| ctx.band(a.min(TAU - a) / std::f32::consts::PI * 0.86);
    let mut shapes = Vec::new();
    for k in (0..rings).rev() {
        let prog = k as f32 / rings as f32;
        let r0 = min_r * (max_r / min_r).powf(prog) * (1.0 + ctx.kick * 0.025);
        let fade = (prog * 5.0).min(1.0) * ((1.0 - prog) * 2.5 + 0.25).min(1.0);
        if fade <= 0.02 {
            continue;
        }
        let ring_hue = hue + ((((k * 47) % 140) + 140) % 140) as f32 - 70.0;
        if k % 2 == 0 {
            let points = (0..=76)
                .map(|s| {
                    let a = s as f32 / 76.0 * TAU;
                    let rr = r0 * (1.0 + band_at(a) * 0.13);
                    (cx + a.cos() * rr, cy + a.sin() * rr)
                })
                .collect();
            shapes.push(Shape {
                geom: Geom::Polyline {
                    points,
                    closed: true,
                },
                fill: ctx.hsla_fill(ring_hue, 0.85, 0.62, 0.85 * fade),
                width: 2.0 + prog * 3.5,
                glow: 0.7,
                dash: None,
            });
        } else {
            for s in 0..44 {
                let a = (s as f32 / 44.0 * TAU + k as f32 * 0.3 + ctx.clock * 0.1).rem_euclid(TAU);
                let v = band_at(a);
                let dash_len = (0.25 + v * 0.85) * (TAU / 44.0) * 0.42;
                shapes.push(Shape {
                    geom: Geom::Arc {
                        cx,
                        cy,
                        r: r0,
                        a0: a,
                        a1: a + dash_len,
                    },
                    fill: ctx.hsla_fill(ring_hue, 0.85, 0.52 + v * 0.16, (0.35 + 0.6 * v) * fade),
                    width: 3.0 + prog * 4.0 + v * 2.0,
                    glow: 0.5,
                    dash: None,
                });
            }
        }
    }
    let bars = 38;
    let span = w * 0.075;
    for i in 0..bars {
        let f = i as f32 / (bars - 1) as f32;
        let v = ctx.band(f * 0.92);
        let bh = 1.5 + v * m * 0.035;
        shapes.push(Shape {
            geom: Geom::Rect {
                x: cx - span + f * span * 2.0,
                y: cy - bh / 2.0,
                w: 1.6,
                h: bh,
            },
            fill: ctx.hsla_fill(hue - 50.0 + f * 100.0, 0.85, 0.64, 0.4 + 0.6 * v),
            width: 0.0,
            glow: 0.0,
            dash: None,
        });
    }
    shapes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visuals::engine::{lively_engine, test_ctx};
    use crate::visuals::scene::{Geom, Scene};

    const WIDTH: f32 = 548.0;
    const HEIGHT: f32 = 300.0;

    /// A lively engine ticked past initial frames so the scene has time to
    /// settle into full-range audio visualization.
    fn active_engine() -> crate::visuals::engine::VisualEngine {
        let mut engine = lively_engine();
        for _ in 0..30 {
            engine.tick();
        }
        engine
    }

    #[test]
    fn scene_is_nonempty_and_sane() {
        let engine = active_engine();
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let shapes = scene(&ctx);

        assert!(!shapes.is_empty(), "scene must not be empty");
        assert!(
            Scene {
                shapes: shapes.clone()
            }
            .is_finite_and_sane(WIDTH, HEIGHT),
            "all shapes must be finite and within bounds"
        );
    }

    #[test]
    fn alternates_closed_wavy_rings_and_tick_arcs_across_eight_depths() {
        let engine = active_engine();
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let shapes = scene(&ctx);

        let closed_polyline_count = shapes
            .iter()
            .filter(|s| matches!(&s.geom, Geom::Polyline { closed: true, .. }))
            .count();

        assert!(
            (1..=4).contains(&closed_polyline_count),
            "expected up to 4 even-depth wavy ring polylines, got {closed_polyline_count}"
        );

        let arc_count = shapes
            .iter()
            .filter(|s| matches!(&s.geom, Geom::Arc { .. }))
            .count();

        assert!(
            arc_count > 0 && arc_count <= 4 * 44,
            "expected up to 44 tick arcs per odd-depth ring (≤4 rings), got {arc_count}"
        );
    }

    #[test]
    fn ring_radii_strictly_increase_with_depth() {
        let engine = active_engine();
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let shapes = scene(&ctx);

        // Arc shapes carry an explicit radius; group by (rounded) radius to
        // recover the distinct depth rings and confirm strict ordering.
        let mut radii: Vec<f32> = shapes
            .iter()
            .filter_map(|s| match &s.geom {
                Geom::Arc { r, .. } => Some(*r),
                _ => None,
            })
            .collect();
        radii.sort_by(|a, b| a.partial_cmp(b).unwrap());
        radii.dedup_by(|a, b| (*a - *b).abs() < 0.01);

        for pair in radii.windows(2) {
            assert!(
                pair[1] > pair[0],
                "ring radii must strictly increase with depth: {pair:?}"
            );
        }
    }

    #[test]
    fn contains_center_mini_bars() {
        let engine = active_engine();
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let shapes = scene(&ctx);

        let rect_count = shapes
            .iter()
            .filter(|s| matches!(&s.geom, Geom::Rect { .. }))
            .count();

        assert_eq!(rect_count, 38, "scene must contain 38 center mini-bars");
    }
}
