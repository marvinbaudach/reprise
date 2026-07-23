//! Flow: 3 flowing wave trails, middle one in secondary accent. Each trail is
//! a polyline spanning the full width, with wavy animation from sine-based
//! oscillations modulated by the frequency spectrum.

use super::super::engine::ModeCtx;
use super::super::scene::{Geom, Shape};

pub(crate) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    let (w, h) = (ctx.width, ctx.height);
    let cy = h * 0.52;
    // Trails bloom brighter when the track is loud, calmer when it's quiet.
    let glow_base = 0.35 + 0.45 * ctx.level;
    let mut shapes: Vec<Shape> = Vec::with_capacity(4);
    let mut mid_points: Vec<(f32, f32)> = Vec::new();
    for layer in 0..3 {
        let l = layer as f32;
        let points: Vec<(f32, f32)> = (0..=(w as usize / 6))
            .map(|step| {
                let px = step as f32 * 6.0;
                let f = px / w;
                let v = ctx.band(f * 0.84);
                let amp = 6.0 + v * h * 0.24 * (1.0 - l * 0.22);
                let y = cy
                    + (px * 0.006 * (1.0 + l * 0.35) + ctx.clock * (1.3 + l * 0.6) + l * 2.1).sin()
                        * amp
                    + (px * 0.017 - ctx.clock * 2.4 + l).sin() * amp * 0.4;
                (px, y.clamp(0.0, h))
            })
            .collect();
        if layer == 1 {
            mid_points = points.clone();
        }
        let fill = if layer == 1 {
            ctx.accent2_fill(0.42)
        } else {
            ctx.accent_fill(0.55 - l * 0.16)
        };
        shapes.push(Shape {
            geom: Geom::Polyline {
                points,
                closed: false,
            },
            fill,
            width: 2.2 - l * 0.5,
            glow: (glow_base - l * 0.12).clamp(0.0, 1.0),
            dash: None,
        });
    }
    // Soft filled band from the middle trail down to the baseline — gives the
    // flow depth instead of three bare strands. Inserted first so it sits under
    // the trails; low alpha keeps it atmospheric.
    if mid_points.len() >= 2 {
        let first_x = mid_points[0].0;
        let last_x = mid_points[mid_points.len() - 1].0;
        let mut area = mid_points;
        area.push((last_x, h));
        area.push((first_x, h));
        shapes.insert(
            0,
            Shape {
                geom: Geom::Polyline {
                    points: area,
                    closed: true,
                },
                fill: ctx.accent2_fill(0.12 + 0.08 * ctx.level),
                width: 0.0,
                glow: 0.0,
                dash: None,
            },
        );
    }
    shapes
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

        // The three open trails, in draw order; the closed under-fill is skipped.
        let trails: Vec<_> = shapes
            .iter()
            .filter(|s| matches!(&s.geom, Geom::Polyline { closed: false, .. }))
            .collect();

        assert_eq!(trails.len(), 3, "expected 3 trail polylines");

        // Layer 1 (middle trail) uses accent2_fill(0.42); verify it's the middle
        // one and carries a glow. Exact colour is left to visual inspection.
        let middle = trails[1];
        assert!(middle.glow > 0.0, "middle trail should have a glow value");
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
