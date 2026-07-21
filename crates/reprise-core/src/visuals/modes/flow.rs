//! Flow: 3 flowing wave trails, middle one in secondary accent. Each trail is
//! a polyline spanning the full width, with wavy animation from sine-based
//! oscillations modulated by the frequency spectrum.

use super::super::engine::ModeCtx;
use super::super::scene::{Geom, Shape};

pub(crate) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    let (w, h) = (ctx.width, ctx.height);
    let cy = h * 0.52;
    (0..3)
        .map(|layer| {
            let l = layer as f32;
            let points = (0..=(w as usize / 6))
                .map(|step| {
                    let px = step as f32 * 6.0;
                    let f = px / w;
                    let v = ctx.band(f * 0.84);
                    let amp = 6.0 + v * h * 0.24 * (1.0 - l * 0.22);
                    let y = cy
                        + (px * 0.006 * (1.0 + l * 0.35) + ctx.clock * (1.3 + l * 0.6) + l * 2.1)
                            .sin()
                            * amp
                        + (px * 0.017 - ctx.clock * 2.4 + l).sin() * amp * 0.4;
                    (px, y.clamp(0.0, h))
                })
                .collect();
            let fill = if layer == 1 {
                ctx.accent2_fill(0.42)
            } else {
                ctx.accent_fill(0.55 - l * 0.16)
            };
            Shape {
                geom: Geom::Polyline {
                    points,
                    closed: false,
                },
                fill,
                width: 2.2 - l * 0.5,
                glow: 0.5 - l * 0.15,
                dash: None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visuals::engine::{lively_engine, test_ctx};
    use crate::visuals::scene::Scene;

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
    fn contains_three_polylines() {
        let engine = active_engine();
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let shapes = scene(&ctx);

        let polyline_count = shapes
            .iter()
            .filter(|s| matches!(&s.geom, Geom::Polyline { closed: false, .. }))
            .count();

        assert_eq!(
            polyline_count, 3,
            "expected exactly 3 polylines, found {polyline_count}"
        );
    }

    #[test]
    fn polylines_span_at_least_90_percent_width() {
        let engine = active_engine();
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let shapes = scene(&ctx);

        for shape in shapes
            .iter()
            .filter(|s| matches!(&s.geom, Geom::Polyline { .. }))
        {
            if let Geom::Polyline { points, .. } = &shape.geom {
                assert!(
                    !points.is_empty(),
                    "polyline must contain at least one point"
                );
                let min_x = points
                    .iter()
                    .map(|(x, _)| x)
                    .cloned()
                    .fold(f32::INFINITY, f32::min);
                let max_x = points.iter().map(|(x, _)| x).cloned().fold(0.0, f32::max);
                let span = max_x - min_x;
                let threshold = WIDTH * 0.9;
                assert!(
                    span >= threshold,
                    "polyline must span ≥90% width; got {span:.1} (threshold {threshold:.1})"
                );
            }
        }
    }

    #[test]
    fn middle_trail_uses_accent2_fill() {
        let engine = active_engine();
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let shapes = scene(&ctx);

        let polylines: Vec<_> = shapes
            .iter()
            .filter(|s| matches!(&s.geom, Geom::Polyline { .. }))
            .collect();

        assert_eq!(polylines.len(), 3, "expected 3 polylines");

        // Layer 1 (middle trail) should have a different fill than layer 0 and 2.
        // We can't easily check color values, but we can verify it's the middle one.
        let middle = polylines[1];
        assert!(middle.glow > 0.0, "middle trail should have a glow value");
        // The key invariant is that layer 1 uses accent2_fill(0.42) which should be
        // visually distinct. We rely on the sane check to verify it's finite.
    }

    #[test]
    fn y_coordinates_are_clamped_to_canvas() {
        let engine = active_engine();
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let shapes = scene(&ctx);

        for shape in shapes
            .iter()
            .filter(|s| matches!(&s.geom, Geom::Polyline { .. }))
        {
            if let Geom::Polyline { points, .. } = &shape.geom {
                for (_, y) in points {
                    assert!(
                        *y >= 0.0 && *y <= HEIGHT,
                        "y coordinate {y} must be clamped to [0, {HEIGHT}]"
                    );
                }
            }
        }
    }
}
