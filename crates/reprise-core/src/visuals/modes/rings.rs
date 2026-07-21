//! Rings: concentric band rings radiating from center, with shockwaves and a
//! core radial glow. 7 band-rings span the frequency spectrum, each growing
//! brighter and thicker as its frequency band rises. Shockwaves expand on
//! impact, and the core pulses with kicks.

use super::super::engine::ModeCtx;
use super::super::scene::{Geom, Shape};
use std::f32::consts::TAU;

pub(crate) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    let (w, h) = (ctx.width, ctx.height);
    let m = w.min(h);
    let (cx, cy) = (w / 2.0, h * 0.46);
    let mut shapes = Vec::new();

    // Shockwaves: expanding rings from impacts (accent2).
    for wave in ctx.impact.shockwaves() {
        shapes.push(Shape {
            geom: Geom::Arc {
                cx,
                cy,
                r: m * 0.12 + wave.progress * m * 0.55,
                a0: 0.0,
                a1: TAU,
            },
            fill: ctx.accent2_fill((1.0 - wave.progress) * wave.strength * 0.5),
            width: 1.5,
            glow: 0.0,
            dash: None,
        });
    }

    // 7 band rings: concentric arcs whose radius and opacity vary with
    // frequency amplitude.
    for i in 0..7 {
        let v = ctx.band(i as f32 / 7.0 * 0.625 + 0.04);
        shapes.push(Shape {
            geom: Geom::Arc {
                cx,
                cy,
                r: m * (0.07 + i as f32 * 0.052) + v * m * 0.075,
                a0: 0.0,
                a1: TAU,
            },
            fill: ctx.accent_fill(0.14 + 0.6 * v),
            width: 2.0 + v * 2.5,
            glow: v * 0.6,
            dash: None,
        });
    }

    // Core radial glow: pulses with the overall level and kicks.
    shapes.push(Shape {
        geom: Geom::RadialGlow {
            cx,
            cy,
            r: m * 0.07 + ctx.level * m * 0.05,
        },
        fill: ctx.accent_fill(0.5 + 0.4 * ctx.kick),
        width: 0.0,
        glow: 0.0,
        dash: None,
    });

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
    fn contains_seven_band_ring_arcs() {
        let engine = active_engine();
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let shapes = scene(&ctx);

        let ring_count = shapes
            .iter()
            .filter(|s| matches!(&s.geom, Geom::Arc { a0, a1, .. } if (*a0 - 0.0).abs() < 1e-6 && (*a1 - TAU).abs() < 1e-6))
            .filter(|s| s.width >= 2.0 && s.width <= 4.5) // Band rings: 2.0 to 2.0+2.5
            .count();

        // We expect at least 7 arcs (the band rings); there may be more if
        // shockwaves are present. This filter isolates rings by checking that
        // width is in the band-ring range.
        assert!(
            ring_count >= 7,
            "expected at least 7 band-ring arcs, found {ring_count}"
        );
    }

    #[test]
    fn contains_a_radial_glow_core() {
        let engine = active_engine();
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let shapes = scene(&ctx);

        let has_core = shapes
            .iter()
            .any(|s| matches!(&s.geom, Geom::RadialGlow { .. }));
        assert!(has_core, "expected 1 RadialGlow core");
    }

    #[test]
    fn shockwaves_are_present_when_impact_generates_them() {
        let engine = active_engine();
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);

        // Confirm we have some shockwave data to draw.
        let wave_count = ctx.impact.shockwaves().count();
        if wave_count > 0 {
            let shapes = scene(&ctx);
            let shockwave_arcs = shapes
                .iter()
                .filter(|s| {
                    matches!(&s.geom, Geom::Arc { a0, a1, .. } if (*a0 - 0.0).abs() < 1e-6 && (*a1 - TAU).abs() < 1e-6)
                })
                .filter(|s| s.width == 1.5) // Shockwaves: width 1.5
                .count();
            assert!(shockwave_arcs > 0, "expected shockwave arcs");
        }
    }
}
