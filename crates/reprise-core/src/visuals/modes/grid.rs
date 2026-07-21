//! Grid: a perspective water surface with dual-accent crests — gray polylines
//! trace every row plus a vertical every 4 columns, and a secondary-accent
//! stroke rides any contiguous run of cells thrown above `0.5` ("splash
//! caught mid-crest").

use super::super::engine::ModeCtx;
use super::super::scene::{Fill, Geom, Rgba, Shape};
use super::super::water::{WATER_COLS, WATER_ROWS};

pub(crate) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    let (w, h) = (ctx.width, ctx.height);
    let horizon = h * 0.30;
    let near_y = h * 0.94;
    let amp = h * 0.26;
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
    // Accent2 on crests thrown above 0.5 — open a segment per contiguous run.
    for (row, (points, near)) in rows.iter().enumerate() {
        let mut run: Vec<(f32, f32)> = Vec::new();
        for (col, &point) in points.iter().enumerate() {
            if ctx.water.height(row, col) > 0.5 {
                run.push(point);
            } else if run.len() >= 2 {
                shapes.push(crest(std::mem::take(&mut run), *near, ctx));
            } else {
                run.clear();
            }
        }
        if run.len() >= 2 {
            shapes.push(crest(run, *near, ctx));
        }
    }
    shapes
}

fn crest(points: Vec<(f32, f32)>, near: f32, ctx: &ModeCtx) -> Shape {
    Shape {
        geom: Geom::Polyline {
            points,
            closed: false,
        },
        fill: ctx.accent2_fill(0.28 + 0.5 * near),
        width: 1.7,
        glow: 0.8,
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
    /// surface has time to actually rise past the `0.5` crest threshold in
    /// multiple cells (the brief's crest invariant needs cells > 0.5, and the
    /// spring-mesh takes more than the initial 10 loud frames to get there).
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
    fn crests_appear_where_the_water_exceeds_half_height() {
        let engine = crested_engine();

        // Confirm the fixture actually drove the surface past the crest
        // threshold somewhere — otherwise the crest-count assertion below
        // would pass vacuously.
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let has_crest_cell = (0..WATER_ROWS)
            .flat_map(|row| (0..WATER_COLS).map(move |col| (row, col)))
            .any(|(row, col)| ctx.water.height(row, col) > 0.5);
        assert!(has_crest_cell, "fixture must drive some cell above 0.5");

        let shapes = scene(&ctx);
        let crest_segments = shapes
            .iter()
            .filter(|s| s.width == 1.7 && s.glow == 0.8)
            .count();
        assert!(
            crest_segments > 0,
            "expected accent2 crest segments where water > 0.5"
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
