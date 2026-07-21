//! Pulse: bass-driven central core with shockwaves and orbiting dots.
//! The core pulse responds to level and kick, surrounded by 16 orbiting dots
//! driven by frequency bands. Shockwaves expand on impact from the accent2 color.

use super::super::engine::ModeCtx;
use super::super::scene::{Geom, Shape};
use std::f32::consts::TAU;

pub(crate) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    let (w, h) = (ctx.width, ctx.height);
    let m = w.min(h);
    let (cx, cy) = (w / 2.0, h * 0.47);
    let mut shapes = Vec::new();

    // Shockwaves: expanding rings from impacts (accent2).
    for wave in ctx.impact.shockwaves() {
        shapes.push(Shape {
            geom: Geom::Arc {
                cx,
                cy,
                r: m * 0.15 + wave.progress * m * 0.55,
                a0: 0.0,
                a1: TAU,
            },
            fill: ctx.accent2_fill((1.0 - wave.progress) * wave.strength * 0.4),
            width: 2.0,
            glow: 0.0,
            dash: None,
        });
    }

    // Core radial glow.
    let r = m * 0.13 + ctx.level * m * 0.10 + ctx.kick * m * 0.04;
    shapes.push(Shape {
        geom: Geom::RadialGlow { cx, cy, r: r * 1.9 },
        fill: ctx.accent_fill(0.4),
        width: 0.0,
        glow: 0.0,
        dash: None,
    });

    // Core arc pulse.
    shapes.push(Shape {
        geom: Geom::Arc {
            cx,
            cy,
            r,
            a0: 0.0,
            a1: TAU,
        },
        fill: ctx.accent_fill(0.85),
        width: 2.5,
        glow: 0.7,
        dash: None,
    });

    // 16 orbiting dots (accent2), positioned by angle and driven by frequency bands.
    for i in 0..16 {
        let angle = i as f32 / 16.0 * TAU + ctx.clock * 0.55;
        let v = ctx.band(0.075 + i as f32 * 0.057);
        let orbit = r + m * 0.05 + v * m * 0.13;
        shapes.push(Shape {
            geom: Geom::Disc {
                cx: cx + angle.cos() * orbit,
                cy: cy + angle.sin() * orbit,
                r: 2.2 + v * 4.5,
            },
            fill: ctx.accent2_fill(0.35 + 0.6 * v),
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
    fn contains_radial_glow_core() {
        let engine = active_engine();
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let shapes = scene(&ctx);

        let has_core = shapes
            .iter()
            .any(|s| matches!(&s.geom, Geom::RadialGlow { .. }));
        assert!(has_core, "expected 1 RadialGlow core");
    }

    #[test]
    fn contains_core_arc() {
        let engine = active_engine();
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let shapes = scene(&ctx);

        let core_arcs = shapes
            .iter()
            .filter(|s| {
                matches!(&s.geom, Geom::Arc { a0, a1, .. } if (*a0 - 0.0).abs() < 1e-6 && (*a1 - TAU).abs() < 1e-6)
            })
            .filter(|s| (s.width - 2.5).abs() < 0.1) // Core arc: width 2.5
            .count();

        assert!(
            core_arcs >= 1,
            "expected at least 1 core arc (width ~2.5), found {core_arcs}"
        );
    }

    #[test]
    fn contains_sixteen_orbit_discs() {
        let engine = active_engine();
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let shapes = scene(&ctx);

        let disc_count = shapes
            .iter()
            .filter(|s| matches!(&s.geom, Geom::Disc { .. }))
            .count();

        assert_eq!(
            disc_count, 16,
            "expected exactly 16 orbit discs, found {disc_count}"
        );
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
                .filter(|s| (s.width - 2.0).abs() < 0.1) // Shockwaves: width 2.0
                .count();
            assert!(shockwave_arcs > 0, "expected shockwave arcs");
        }
    }
}
