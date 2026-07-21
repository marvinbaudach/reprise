//! Neon: hue-sweep segment meter with dashed envelope lines.
//! Renders a series of colored bars with a lively, animated outline that
//! pulses and oscillates above and below the centerline.

use super::super::color;
use super::super::engine::ModeCtx;
use super::super::scene::{Geom, Shape};

pub(crate) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    let (w, h) = (ctx.width, ctx.height);
    let cy = h * 0.5;
    let hue = color::rgb_hue(ctx.accent);
    let mut shapes = Vec::new();
    let seg = (w * 0.008).max(9.0);
    let gap = seg * 0.8;
    let n = ((w * 0.8) / (seg + gap)) as usize;
    for i in 0..n {
        let f = i as f32 / (n - 1).max(1) as f32;
        let v = ctx.band(f * 0.92);
        let bh = 3.0 + v * h * 0.065;
        shapes.push(Shape {
            geom: Geom::Rect {
                x: w * 0.1 + i as f32 * (seg + gap),
                y: cy - bh / 2.0,
                w: seg * 0.55,
                h: bh,
            },
            fill: ctx.hue_sweep_fill(hue, 0.9, w * 0.08, w * 0.92),
            width: 0.0,
            glow: 0.4,
            dash: None,
        });
    }
    let lines = [
        (-h * 0.075, 0.8_f32, None, 1.0_f32),
        (h * 0.075, 0.8, None, 1.0),
        (-h * 0.135, 0.45, Some((2.0, 6.0)), 0.7),
        (h * 0.135, 0.45, Some((2.0, 6.0)), 0.7),
    ];
    for (li, (off, alpha, dash, amp)) in lines.into_iter().enumerate() {
        let sign = if off < 0.0 { -1.0 } else { 1.0 };
        let points = (0..=((w * 0.84) as usize / 5))
            .map(|step| {
                let px = w * 0.08 + step as f32 * 5.0;
                let f = (px - w * 0.08) / (w * 0.84);
                let v = ctx.band(f * 0.92);
                let y = cy + off
                    - sign * v.powf(1.6) * h * 0.05 * amp
                    - sign * (px * 0.05 + ctx.clock * 4.0 + li as f32).sin() * v * h * 0.012;
                (px, y)
            })
            .collect();
        shapes.push(Shape {
            geom: Geom::Polyline {
                points,
                closed: false,
            },
            fill: ctx.hue_sweep_fill(hue, alpha, w * 0.08, w * 0.92),
            width: if dash.is_some() { 1.2 } else { 1.8 },
            glow: if dash.is_some() { 0.3 } else { 0.6 },
            dash,
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
    fn contains_segment_rects() {
        let engine = active_engine();
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let shapes = scene(&ctx);

        let rect_count = shapes
            .iter()
            .filter(|s| matches!(&s.geom, Geom::Rect { .. }))
            .count();

        assert!(
            rect_count > 0,
            "scene must contain at least one segment Rect"
        );
    }

    #[test]
    fn contains_envelope_polylines() {
        let engine = active_engine();
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let shapes = scene(&ctx);

        let polyline_count = shapes
            .iter()
            .filter(|s| matches!(&s.geom, Geom::Polyline { .. }))
            .count();

        assert_eq!(
            polyline_count, 4,
            "scene must contain exactly 4 envelope polylines"
        );
    }

    #[test]
    fn outer_envelope_polylines_are_dashed() {
        let engine = active_engine();
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let shapes = scene(&ctx);

        let mut polyline_dashes: Vec<Option<(f32, f32)>> = shapes
            .iter()
            .filter_map(|s| {
                if matches!(&s.geom, Geom::Polyline { .. }) {
                    Some(s.dash)
                } else {
                    None
                }
            })
            .collect();

        // Sort to ensure consistent order
        polyline_dashes.sort_by(|a, b| {
            let a_val = a.map_or(f32::NAN, |(on, _)| on);
            let b_val = b.map_or(f32::NAN, |(on, _)| on);
            a_val
                .partial_cmp(&b_val)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // First two (undashed), last two (dashed with (2.0, 6.0))
        assert_eq!(
            polyline_dashes[0], None,
            "first polyline (inner upper) should not be dashed"
        );
        assert_eq!(
            polyline_dashes[1], None,
            "second polyline (inner lower) should not be dashed"
        );
        assert_eq!(
            polyline_dashes[2],
            Some((2.0, 6.0)),
            "third polyline (outer upper) should be dashed (2.0, 6.0)"
        );
        assert_eq!(
            polyline_dashes[3],
            Some((2.0, 6.0)),
            "fourth polyline (outer lower) should be dashed (2.0, 6.0)"
        );
    }

    #[test]
    fn polylines_have_reasonable_point_counts() {
        let engine = active_engine();
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let shapes = scene(&ctx);

        let polylines: Vec<_> = shapes
            .iter()
            .filter_map(|s| {
                if let Geom::Polyline { points, .. } = &s.geom {
                    Some(points)
                } else {
                    None
                }
            })
            .collect();

        for (i, points) in polylines.iter().enumerate() {
            assert!(
                !points.is_empty(),
                "polyline {i} must have at least one point"
            );
            assert!(
                points.len() > 5,
                "polyline {i} should have enough points for smooth curves"
            );
        }
    }
}
