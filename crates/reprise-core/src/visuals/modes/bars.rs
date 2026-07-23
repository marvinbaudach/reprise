//! Bars: the flagship mode — one polyline per display band whose colour blends
//! smoothly from the primary accent up into the hotter secondary accent as a
//! band peaks, a floating peak-hold cap per band (classic analyzer read-out),
//! plus spark particles radiating from centre on every beat.

use crate::playback::SPECTRUM_BAND_COUNT;

use super::super::engine::ModeCtx;
use super::super::scene::{Fill, Geom, Rgba, Shape};

/// Bar height as a fraction of canvas height at a full-scale band.
const BAR_SCALE: f32 = 0.8;
/// Band value where the colour starts blending toward the hot secondary accent;
/// fully there at 1.0. A smooth ramp instead of a hard flip.
const HOT_BLEND_START: f32 = 0.55;
/// Peak-hold caps below this read as silence and are skipped.
const PEAK_MIN: f32 = 0.03;

pub(crate) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    let (w, h) = (ctx.width, ctx.height);
    let m = w.min(h);
    let n = SPECTRUM_BAND_COUNT; // 64 columns, 1:1 with display bands
    let col_w = w / n as f32;
    let (ar, ag, ab) = ctx.accent;
    let (br, bg, bb) = ctx.accent2;
    let mut shapes: Vec<Shape> = (0..n)
        .map(|i| {
            let v = ctx.bands[i];
            let px = (i as f32 + 0.5) * col_w;
            let len = (v * h * BAR_SCALE).max(4.0);
            // Blend accent → accent2 as the band climbs into peak territory, so
            // the "hot" colour eases in rather than snapping at a threshold.
            let t = ((v - HOT_BLEND_START) / (1.0 - HOT_BLEND_START)).clamp(0.0, 1.0);
            let fill = Fill::Solid(Rgba {
                r: ar + (br - ar) * t,
                g: ag + (bg - ag) * t,
                b: ab + (bb - ab) * t,
                a: 0.30 + 0.60 * v,
            });
            Shape {
                geom: Geom::Polyline {
                    points: vec![(px, h - 2.0), (px, h - len)],
                    closed: false,
                },
                fill,
                width: (m * 0.006).max(4.0),
                glow: v,
                dash: None,
            }
        })
        .collect();
    // Peak-hold caps: a bright marker floating at each band's recent peak,
    // decaying slowly (engine PEAK_DECAY) — the classic spectrum-analyzer look
    // and a clear read-out of true loudness.
    for i in 0..n {
        let pv = ctx.peaks[i];
        if pv <= PEAK_MIN {
            continue;
        }
        let px = (i as f32 + 0.5) * col_w;
        let py = h - (pv * h * BAR_SCALE).max(4.0);
        let half = col_w * 0.4;
        shapes.push(Shape {
            geom: Geom::Polyline {
                points: vec![(px - half, py), (px + half, py)],
                closed: false,
            },
            fill: Fill::Solid(Rgba {
                r: 0.90,
                g: 0.93,
                b: 0.97,
                a: 0.35 + 0.45 * pv,
            }),
            width: (m * 0.005).max(2.0),
            glow: 0.3,
            dash: None,
        });
    }
    for spark in ctx.impact.particles() {
        shapes.push(Shape {
            geom: Geom::Disc {
                cx: w / 2.0 + spark.angle.cos() * spark.dist,
                cy: h / 2.0 + spark.angle.sin() * spark.dist,
                r: 1.4 + spark.life_frac * 2.6,
            },
            fill: ctx.accent_fill(spark.life_frac),
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

    /// A lively engine ticked past initial frames so the bars sit at full range
    /// and every band's peak-hold is seeded.
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
    fn draws_one_vertical_bar_per_display_band() {
        let engine = active_engine();
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let shapes = scene(&ctx);
        // Bars are the vertical 2-point polylines (both endpoints share x).
        let bars = shapes
            .iter()
            .filter(|s| {
                matches!(&s.geom, Geom::Polyline { points, .. }
                    if points.len() == 2 && (points[0].0 - points[1].0).abs() < 1e-6)
            })
            .count();
        assert_eq!(
            bars, SPECTRUM_BAND_COUNT,
            "expected one vertical bar per display band"
        );
    }

    #[test]
    fn peak_hold_caps_float_above_the_bars() {
        // The lively fixture seeds every band's peak-hold high, so the bright
        // horizontal cap markers must be drawn.
        let engine = active_engine();
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let shapes = scene(&ctx);
        let caps = shapes
            .iter()
            .filter(|s| {
                let horizontal = matches!(&s.geom, Geom::Polyline { points, .. }
                    if points.len() == 2 && (points[0].1 - points[1].1).abs() < 1e-6);
                let bright = matches!(&s.fill, Fill::Solid(Rgba { r, g, b, .. })
                    if (*r - 0.90).abs() < 1e-6 && (*g - 0.93).abs() < 1e-6 && (*b - 0.97).abs() < 1e-6);
                horizontal && bright
            })
            .count();
        assert!(caps > 0, "expected peak-hold cap markers, found {caps}");
    }
}
