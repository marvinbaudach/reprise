//! Bars: the flagship mode — one polyline per display band, dual-accent
//! (a hotter secondary accent kicks in past `0.66`), plus spark particles
//! radiating from center on every beat.

use crate::playback::SPECTRUM_BAND_COUNT;

use super::super::engine::ModeCtx;
use super::super::scene::{Geom, Shape};

pub(crate) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    let (w, h) = (ctx.width, ctx.height);
    let m = w.min(h);
    let n = SPECTRUM_BAND_COUNT; // 64 columns, 1:1 with display bands
    let mut shapes: Vec<Shape> = (0..n)
        .map(|i| {
            let v = ctx.bands[i];
            let px = (i as f32 + 0.5) * w / n as f32;
            let len = (v * h * 0.8).max(4.0);
            let fill = if v > 0.66 {
                ctx.accent2_fill(0.3 + 0.6 * v)
            } else {
                ctx.accent_fill(0.28 + 0.62 * v)
            };
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
