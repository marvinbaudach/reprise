//! Grid: a perspective water surface with dual-accent crests — gray polylines
//! trace every row plus a vertical every 4 columns, and a secondary-accent
//! stroke rides any contiguous run of cells thrown above `0.5` ("splash
//! caught mid-crest").

use super::super::engine::ModeCtx;
use super::super::scene::{Fill, Geom, Rgba, Shape};
use super::super::water::{WATER_COLS, WATER_ROWS};

/// Water height at which a crest starts to show (secondary accent). Set high so
/// the "hot" crest only rides genuine peaks and beat eruptions, not the modest
/// ripples of ordinary playback.
const CREST_THRESHOLD: f32 = 0.85;
/// Height at which a crest reads full-intensity. Between the threshold and this
/// the crest fades in, so the hot colour grows with the peak instead of snapping
/// on all at once.
const CREST_FULL: f32 = 2.0;

pub(crate) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    let (w, h) = (ctx.width, ctx.height);
    let horizon = h * 0.30;
    let near_y = h * 0.94;
    let amp = h * 0.32;
    // Project every water cell once.
    let mut rows: Vec<(Vec<(f32, f32)>, f32)> = Vec::with_capacity(WATER_ROWS); // (points, near)
    for row in 0..WATER_ROWS {
        let near = (row as f32 / (WATER_ROWS - 1) as f32).powf(1.6);
        let y0 = horizon + near * (near_y - horizon);
        let half = w * 0.30 + near * w * 0.68;
        let row_amp = amp * (0.35 + 0.65 * near);
        let points = (0..WATER_COLS)
            .map(|col| {
                let px = w / 2.0 + ((col as f32 / (WATER_COLS - 1) as f32) - 0.5) * 2.0 * half;
                (
                    px,
                    (y0 - ctx.water.height(row, col) * row_amp).clamp(0.0, h),
                )
            })
            .collect();
        rows.push((points, near));
    }
    let gray = |alpha: f32| {
        Fill::Solid(Rgba {
            r: 0.815,
            g: 0.831,
            b: 0.894,
            a: alpha,
        })
    };
    let mut shapes = Vec::new();
    for (points, near) in &rows {
        shapes.push(Shape {
            geom: Geom::Polyline {
                points: points.clone(),
                closed: false,
            },
            fill: gray(0.10 + 0.40 * near),
            width: 1.0 + near * 0.8,
            glow: 0.0,
            dash: None,
        });
    }
    for col in (0..WATER_COLS).step_by(4) {
        shapes.push(Shape {
            geom: Geom::Polyline {
                points: rows.iter().map(|(p, _)| p[col]).collect(),
                closed: false,
            },
            fill: gray(0.14),
            width: 1.0,
            glow: 0.0,
            dash: None,
        });
    }
    // Secondary-accent crests ride any contiguous run of cells thrown above
    // CREST_THRESHOLD. Brightness/width fade in with how high the run peaked, so
    // a crest only reads "hot" on a genuine eruption, not on every ripple.
    for (row, (points, near)) in rows.iter().enumerate() {
        let mut run: Vec<(f32, f32)> = Vec::new();
        let mut run_peak = 0.0_f32;
        for (col, &point) in points.iter().enumerate() {
            let height = ctx.water.height(row, col);
            if height > CREST_THRESHOLD {
                run.push(point);
                run_peak = run_peak.max(height);
            } else if run.len() >= 2 {
                shapes.push(crest(std::mem::take(&mut run), run_peak, *near, ctx));
                run_peak = 0.0;
            } else {
                run.clear();
                run_peak = 0.0;
            }
        }
        if run.len() >= 2 {
            shapes.push(crest(run, run_peak, *near, ctx));
        }
    }
    shapes
}

fn crest(points: Vec<(f32, f32)>, peak: f32, near: f32, ctx: &ModeCtx) -> Shape {
    let intensity = ((peak - CREST_THRESHOLD) / (CREST_FULL - CREST_THRESHOLD)).clamp(0.0, 1.0);
    Shape {
        geom: Geom::Polyline {
            points,
            closed: false,
        },
        fill: ctx.accent2_fill((0.16 + 0.34 * near) * (0.4 + 0.6 * intensity)),
        width: 1.3 + 0.7 * intensity,
        glow: 0.45 + 0.4 * intensity,
        dash: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visuals::engine::{lively_engine, test_ctx};
    use crate::visuals::scene::Scene;

    const WIDTH: f32 = 548.0;
    const HEIGHT: f32 = 300.0;

    /// A lively engine, ticked well past the initial beat/slam so the water
    /// surface has time to rise past the crest threshold in multiple cells (the
    /// crest invariant needs cells above [`CREST_THRESHOLD`], and the driven hot
    /// zone climbs there within a few frames of sustained loud input).
    fn crested_engine() -> crate::visuals::engine::VisualEngine {
        let mut engine = lively_engine();
        for _ in 0..40 {
            engine.tick();
        }
        engine
    }

    #[test]
    fn scene_is_nonempty_and_sane() {
        let engine = crested_engine();
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let shapes = scene(&ctx);
        assert!(!shapes.is_empty());
        assert!(Scene {
            shapes: shapes.clone()
        }
        .is_finite_and_sane(WIDTH, HEIGHT));
    }

    /// The mesh is gray (r≈0.815, g≈0.831, b≈0.894); crests use the
    /// secondary accent instead, so this is what tells a row/vertical mesh
    /// line apart from a crest segment that happens to span the same number
    /// of points (e.g. a fully-crested row is also 44 points long).
    fn is_gray_mesh_line(shape: &Shape) -> bool {
        matches!(
            &shape.fill,
            Fill::Solid(Rgba { r, g, b, .. })
                if (*r - 0.815).abs() < 1e-6 && (*g - 0.831).abs() < 1e-6 && (*b - 0.894).abs() < 1e-6
        )
    }

    #[test]
    fn draws_a_row_polyline_per_water_row_plus_verticals_every_4_cols() {
        let engine = crested_engine();
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let shapes = scene(&ctx);

        let row_lines = shapes
            .iter()
            .filter(|s| {
                is_gray_mesh_line(s)
                    && matches!(&s.geom, Geom::Polyline { points, .. } if points.len() == WATER_COLS)
            })
            .count();
        assert_eq!(row_lines, WATER_ROWS, "one gray polyline per water row");

        let vertical_lines = shapes
            .iter()
            .filter(|s| {
                is_gray_mesh_line(s)
                    && matches!(&s.geom, Geom::Polyline { points, .. } if points.len() == WATER_ROWS)
            })
            .count();
        let expected_verticals = (0..WATER_COLS).step_by(4).count();
        assert_eq!(
            vertical_lines, expected_verticals,
            "one vertical polyline every 4 columns"
        );
    }

    #[test]
    fn crests_appear_where_the_water_exceeds_the_threshold() {
        let engine = crested_engine();

        // Confirm the fixture actually drove the surface past the crest
        // threshold somewhere — otherwise the crest-count assertion below
        // would pass vacuously.
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let has_crest_cell = (0..WATER_ROWS)
            .flat_map(|row| (0..WATER_COLS).map(move |col| (row, col)))
            .any(|(row, col)| ctx.water.height(row, col) > CREST_THRESHOLD);
        assert!(
            has_crest_cell,
            "fixture must drive some cell above the crest threshold"
        );

        let shapes = scene(&ctx);
        // Crests are the only shapes drawn with a non-zero glow; the gray mesh
        // rows and verticals all have glow 0.0.
        let crest_segments = shapes.iter().filter(|s| s.glow > 0.0).count();
        assert!(
            crest_segments > 0,
            "expected accent2 crest segments where water exceeds the threshold"
        );
    }

    #[test]
    fn every_point_y_is_within_the_canvas() {
        let engine = crested_engine();
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let shapes = scene(&ctx);
        for shape in &shapes {
            if let Geom::Polyline { points, .. } = &shape.geom {
                for (_, y) in points {
                    assert!(
                        (0.0..=HEIGHT).contains(y),
                        "y {y} escaped the canvas 0.0..={HEIGHT}"
                    );
                }
            }
        }
    }
}
