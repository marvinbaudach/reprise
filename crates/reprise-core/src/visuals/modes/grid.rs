//! Grid: a dense perspective wire membrane. A modest physics lattice is
//! bilinearly sampled into a 52×76 render mesh, so the cloth reads as thin
//! without multiplying simulation work. A secondary-accent crest and a soft
//! pressure glow illuminate the central bass push.

use super::super::engine::GridCtx;
use super::super::scene::{Fill, Geom, Rgba, Shape};

/// Membrane height at which a crest starts to show (secondary accent). Set high so
/// the "hot" crest only rides genuine peaks and beat eruptions, not the modest
/// ripples of ordinary playback.
const CREST_THRESHOLD: f32 = 0.85;
/// Height at which a crest reads full-intensity. Between the threshold and this
/// the crest fades in, so the hot colour grows with the peak instead of snapping
/// on all at once.
const CREST_FULL: f32 = 2.0;
const RENDER_ROWS: usize = 52;
const RENDER_COLS: usize = 76;
const HORIZON_Y: f32 = 0.22;
const NEAR_Y: f32 = 0.95;
const FAR_HALF_WIDTH: f32 = 0.18;
const NEAR_HALF_WIDTH: f32 = 0.54;
const PERSPECTIVE_POWER: f32 = 1.65;
const HEIGHT_SCALE: f32 = 0.48;

struct ProjectedRow {
    points: Vec<(f32, f32)>,
    heights: Vec<f32>,
    near: f32,
}

fn perspective(depth: f32) -> f32 {
    depth.powf(PERSPECTIVE_POWER)
}

fn project(ctx: &GridCtx, near: f32, across: f32, height: f32) -> (f32, f32) {
    let baseline = ctx.height * (HORIZON_Y + near * (NEAR_Y - HORIZON_Y));
    let half_width = ctx.width * (FAR_HALF_WIDTH + near * (NEAR_HALF_WIDTH - FAR_HALF_WIDTH));
    let vertical_scale = ctx.height * HEIGHT_SCALE * (0.28 + 0.72 * near);
    let x = ctx.width / 2.0 + (across - 0.5) * 2.0 * half_width;
    let y = (baseline - height * vertical_scale).clamp(0.0, ctx.height);
    (x, y)
}

pub(crate) fn scene(ctx: &GridCtx) -> Vec<Shape> {
    let mut rows = Vec::with_capacity(RENDER_ROWS);
    for row in 0..RENDER_ROWS {
        let depth = row as f32 / (RENDER_ROWS - 1) as f32;
        let near = perspective(depth);
        let mut points = Vec::with_capacity(RENDER_COLS);
        let mut heights = Vec::with_capacity(RENDER_COLS);
        for col in 0..RENDER_COLS {
            let across = col as f32 / (RENDER_COLS - 1) as f32;
            let height = ctx.membrane.sample(depth, across);
            points.push(project(ctx, near, across, height));
            heights.push(height);
        }
        rows.push(ProjectedRow {
            points,
            heights,
            near,
        });
    }

    let gray = |alpha: f32| {
        Fill::Solid(Rgba {
            r: 0.815,
            g: 0.831,
            b: 0.894,
            a: alpha,
        })
    };
    let mut shapes = Vec::with_capacity(RENDER_ROWS + RENDER_COLS + 12);

    let pressure = ctx.membrane.pressure();
    if pressure > 0.01 {
        let center_height = ctx.membrane.sample(0.5, 0.5);
        // Place the light inside the lifted dome rather than on its apex, so
        // the wire skin remains visible over a broad illuminated volume.
        let (cx, cy) = project(ctx, perspective(0.5), 0.5, center_height * 0.55);
        let (r, g, b) = ctx.accent2;
        let white_mix = 0.30 + pressure * 0.35;
        shapes.push(Shape {
            geom: Geom::RadialGlow {
                cx,
                cy,
                r: ctx.width * (0.14 + pressure * 0.09),
            },
            fill: Fill::Solid(Rgba {
                r: r + (1.0 - r) * white_mix,
                g: g + (1.0 - g) * white_mix,
                b: b + (1.0 - b) * white_mix,
                a: 0.08 + pressure * 0.24,
            }),
            width: 0.0,
            glow: 0.0,
            dash: None,
        });
    }

    for row in &rows {
        shapes.push(Shape {
            geom: Geom::Polyline {
                points: row.points.clone(),
                closed: false,
            },
            fill: gray(0.08 + 0.34 * row.near),
            width: 0.45 + row.near * 0.55,
            glow: 0.0,
            dash: None,
        });
    }
    for col in 0..RENDER_COLS {
        shapes.push(Shape {
            geom: Geom::Polyline {
                points: rows.iter().map(|row| row.points[col]).collect(),
                closed: false,
            },
            fill: gray(0.15),
            width: 0.55,
            glow: 0.0,
            dash: None,
        });
    }
    // Secondary-accent crests ride any contiguous run of cells thrown above
    // CREST_THRESHOLD. Brightness/width fade in with how high the run peaked, so
    // a crest only reads "hot" on a genuine eruption, not on every ripple.
    for row in &rows {
        let mut run: Vec<(f32, f32)> = Vec::new();
        let mut run_peak = 0.0_f32;
        for (&point, &height) in row.points.iter().zip(row.heights.iter()) {
            if height > CREST_THRESHOLD {
                run.push(point);
                run_peak = run_peak.max(height);
            } else if run.len() >= 2 {
                shapes.push(crest(std::mem::take(&mut run), run_peak, row.near, ctx));
                run_peak = 0.0;
            } else {
                run.clear();
                run_peak = 0.0;
            }
        }
        if run.len() >= 2 {
            shapes.push(crest(run, run_peak, row.near, ctx));
        }
    }
    shapes
}

fn crest(points: Vec<(f32, f32)>, peak: f32, near: f32, ctx: &GridCtx) -> Shape {
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
    use crate::visuals::membrane::{MEMBRANE_COLS, MEMBRANE_ROWS};
    use crate::visuals::scene::Scene;

    const WIDTH: f32 = 548.0;
    const HEIGHT: f32 = 300.0;

    /// A lively engine, ticked well past the initial beat/slam so the membrane
    /// has time to rise past the crest threshold in multiple cells (the
    /// crest invariant needs cells above [`CREST_THRESHOLD`], and the central
    /// driver climbs there within a few frames of sustained loud input).
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
    fn renders_a_dense_52_by_76_wire_mesh() {
        let engine = crested_engine();
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let shapes = scene(&ctx);

        let row_lines = shapes
            .iter()
            .filter(|s| {
                is_gray_mesh_line(s)
                    && matches!(&s.geom, Geom::Polyline { points, .. } if points.len() == 76)
            })
            .count();
        assert_eq!(row_lines, 52, "52 interpolated depth lines");

        let vertical_lines = shapes
            .iter()
            .filter(|s| {
                is_gray_mesh_line(s)
                    && matches!(&s.geom, Geom::Polyline { points, .. } if points.len() == 52)
            })
            .count();
        assert_eq!(vertical_lines, 76, "one line through every rendered column");

        let rendered_points: usize = shapes
            .iter()
            .filter_map(|shape| match &shape.geom {
                Geom::Polyline { points, .. } => Some(points.len()),
                _ => None,
            })
            .sum();
        assert!(
            rendered_points <= 12_000,
            "dense interpolation must stay bounded, got {rendered_points} points"
        );
    }

    #[test]
    fn flat_grid_recedes_toward_a_tighter_horizon() {
        let engine = crate::visuals::engine::VisualEngine::new();
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let shapes = scene(&ctx);
        let rows: Vec<&[(f32, f32)]> = shapes
            .iter()
            .filter_map(|shape| match &shape.geom {
                Geom::Polyline { points, .. } if is_gray_mesh_line(shape) && points.len() == 76 => {
                    Some(points.as_slice())
                }
                _ => None,
            })
            .collect();
        assert_eq!(rows.len(), 52);

        let far_width = rows[0].last().unwrap().0 - rows[0][0].0;
        let near_width = rows[51].last().unwrap().0 - rows[51][0].0;
        assert!(
            near_width > far_width * 2.5,
            "near edge {near_width} must be much wider than horizon {far_width}"
        );
        let far_gap = rows[1][0].1 - rows[0][0].1;
        let near_gap = rows[51][0].1 - rows[50][0].1;
        assert!(
            near_gap > far_gap * 2.0,
            "depth spacing must open toward the viewer: far {far_gap}, near {near_gap}"
        );
    }

    #[test]
    fn positive_bass_pressure_adds_a_center_glow_only_during_the_push() {
        let engine = lively_engine();
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        assert!(ctx.membrane.pressure() > 0.05);
        let shapes = scene(&ctx);
        let glow = shapes.iter().find(|shape| {
            matches!(
                shape.geom,
                Geom::RadialGlow {
                    cx,
                    cy,
                    r
                } if (cx - WIDTH / 2.0).abs() < 1.0
                    && (HEIGHT * 0.15..HEIGHT * 0.80).contains(&cy)
                    && (WIDTH * 0.10..WIDTH * 0.40).contains(&r)
            )
        });
        assert!(glow.is_some(), "positive bass pressure needs a center glow");

        let quiet = crate::visuals::engine::VisualEngine::new();
        let quiet_ctx = test_ctx(&quiet, WIDTH, HEIGHT);
        assert_eq!(quiet_ctx.membrane.pressure(), 0.0);
        assert!(
            !scene(&quiet_ctx)
                .iter()
                .any(|shape| matches!(shape.geom, Geom::RadialGlow { .. })),
            "a resting membrane must not glow"
        );
    }

    #[test]
    fn crests_appear_where_the_membrane_exceeds_the_threshold() {
        let engine = crested_engine();

        // Confirm the fixture actually drove the surface past the crest
        // threshold somewhere — otherwise the crest-count assertion below
        // would pass vacuously.
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let has_crest_cell = (0..MEMBRANE_ROWS)
            .flat_map(|row| (0..MEMBRANE_COLS).map(move |col| (row, col)))
            .any(|(row, col)| ctx.membrane.height(row, col) > CREST_THRESHOLD);
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
            "expected accent2 crest segments where the membrane exceeds the threshold"
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
