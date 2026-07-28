//! Sixty-four finely segmented neon columns driven one-to-one by CAVA bars.

use crate::playback::SPECTRUM_BAND_COUNT;
use crate::visuals::color::hsla_to_rgb;

use super::super::engine::ModeCtx;
use super::super::scene::{Fill, Geom, Rgba, Shape};

pub(crate) const BAR_COUNT: usize = SPECTRUM_BAND_COUNT;
const SEGMENT_COUNT: usize = 16;
const HORIZONTAL_MARGIN: f32 = 0.045;
const BAR_GAP: f32 = 0.0025;
const BASELINE: f32 = 0.82;
const MAX_HEIGHT: f32 = 0.68;
const SEGMENT_GAP: f32 = 2.5;
const PEAK_CAP_HEIGHT: f32 = 2.5;
const PEAK_MIN: f32 = 0.04;
const REFLECTION_SEGMENTS: usize = 6;
const HUE_START: f32 = 188.0;
const HUE_END: f32 = 315.0;
const BASS_GLOW_ALPHA: f32 = 0.35;
const BASS_GLOW_RADIUS: f32 = 0.44;
const BREAKDOWN_GLOW_ALPHA: f32 = 0.55;
const BREAKDOWN_GLOW_RADIUS: f32 = 0.32;

fn smoothstep(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn neon(bar: usize, alpha: f32) -> Fill {
    let across = bar as f32 / (BAR_COUNT - 1) as f32;
    let hue = HUE_START + (HUE_END - HUE_START) * across;
    let (r, g, b) = hsla_to_rgb(hue, 0.88, 0.60);
    Fill::Solid(Rgba { r, g, b, a: alpha })
}

pub(crate) fn scene(ctx: &ModeCtx) -> Vec<Shape> {
    let margin = ctx.width * HORIZONTAL_MARGIN;
    let gap = ctx.width * BAR_GAP;
    let bar_width = (ctx.width - margin * 2.0 - gap * (BAR_COUNT - 1) as f32) / BAR_COUNT as f32;
    let baseline = ctx.height * BASELINE;
    let max_height = ctx.height * MAX_HEIGHT;
    let segment_height =
        (max_height - SEGMENT_GAP * (SEGMENT_COUNT - 1) as f32) / SEGMENT_COUNT as f32;
    let mut shapes = Vec::with_capacity(BAR_COUNT * (SEGMENT_COUNT + REFLECTION_SEGMENTS + 2) + 4);

    // Both layers scale straight off the measured pressure, so a rhythmic kick
    // is a soft lift and only a sustained breakdown reaches the inner auras.
    if ctx.bass_impact > 0.0 {
        let radius = ctx.width.max(ctx.height) * BASS_GLOW_RADIUS;
        for (bar, cx) in [(0, ctx.width * 0.28), (BAR_COUNT - 1, ctx.width * 0.72)] {
            shapes.push(Shape {
                geom: Geom::RadialGlow {
                    cx,
                    cy: ctx.height * 0.68,
                    r: radius,
                },
                fill: neon(bar, BASS_GLOW_ALPHA * ctx.bass_impact),
                width: 0.0,
                glow: 0.0,
                dash: None,
            });
        }
    }

    if ctx.bass_aura > 0.0 {
        let radius = ctx.width.max(ctx.height) * BREAKDOWN_GLOW_RADIUS;
        for (bar, cx) in [
            (BAR_COUNT / 10, ctx.width * 0.32),
            (BAR_COUNT - 1 - BAR_COUNT / 10, ctx.width * 0.68),
        ] {
            shapes.push(Shape {
                geom: Geom::RadialGlow {
                    cx,
                    cy: ctx.height * 0.66,
                    r: radius,
                },
                fill: neon(bar, BREAKDOWN_GLOW_ALPHA * ctx.bass_aura),
                width: 0.0,
                glow: 0.0,
                dash: None,
            });
        }
    }

    for bar in 0..BAR_COUNT {
        let x = margin + bar as f32 * (bar_width + gap);
        let value = ctx.bars[bar];
        let active = (value * SEGMENT_COUNT as f32).ceil() as usize;
        let fraction = value * SEGMENT_COUNT as f32;

        if value > 0.10 {
            let top = baseline - value * max_height;
            shapes.push(Shape {
                geom: Geom::RadialGlow {
                    cx: x + bar_width / 2.0,
                    cy: top + segment_height,
                    r: bar_width * (1.25 + value * 0.55),
                },
                fill: neon(bar, 0.08 + value * 0.13),
                width: 0.0,
                glow: 0.0,
                dash: None,
            });
        }

        for segment in 0..active.min(SEGMENT_COUNT) {
            let transition = smoothstep((fraction - segment as f32).clamp(0.0, 1.0));
            let y = baseline - (segment + 1) as f32 * (segment_height + SEGMENT_GAP);
            shapes.push(Shape {
                geom: Geom::Rect {
                    x,
                    y,
                    w: bar_width,
                    h: segment_height,
                },
                fill: neon(bar, 0.96 * transition),
                width: 0.0,
                glow: 0.0,
                dash: None,
            });

            if segment < REFLECTION_SEGMENTS {
                let reflection_height = segment_height * 0.42;
                shapes.push(Shape {
                    geom: Geom::Rect {
                        x,
                        y: baseline + SEGMENT_GAP + segment as f32 * (reflection_height + 2.0),
                        w: bar_width,
                        h: reflection_height,
                    },
                    fill: neon(bar, (0.13 - segment as f32 * 0.02) * transition),
                    width: 0.0,
                    glow: 0.0,
                    dash: None,
                });
            }
        }

        let peak = ctx.peaks[bar];
        if peak > PEAK_MIN {
            shapes.push(Shape {
                geom: Geom::Rect {
                    x: x + bar_width * 0.08,
                    y: baseline - peak * max_height - PEAK_CAP_HEIGHT,
                    w: bar_width * 0.84,
                    h: PEAK_CAP_HEIGHT,
                },
                fill: Fill::Solid(Rgba {
                    r: 0.94,
                    g: 0.96,
                    b: 1.0,
                    a: 0.38 + peak * 0.48,
                }),
                width: 0.0,
                glow: 0.0,
                dash: None,
            });
        }
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

    fn main_segments(shapes: &[Shape]) -> Vec<&Shape> {
        let baseline = HEIGHT * BASELINE;
        shapes
            .iter()
            .filter(|shape| {
                matches!(
                    shape.geom,
                    Geom::Rect { y, h, .. }
                        if y < baseline && h > PEAK_CAP_HEIGHT + 0.5
                )
            })
            .collect()
    }

    #[test]
    fn scene_is_nonempty_and_sane() {
        let shapes = scene(&test_ctx(&lively_engine(), WIDTH, HEIGHT));
        assert!(!shapes.is_empty());
        assert!(Scene {
            shapes: shapes.clone()
        }
        .is_finite_and_sane(WIDTH, HEIGHT));
    }

    #[test]
    fn ac_23_draws_sixty_four_one_to_one_cava_columns() {
        let engine = lively_engine();
        let shapes = scene(&test_ctx(&engine, WIDTH, HEIGHT));
        let mut x_positions: Vec<f32> = main_segments(&shapes)
            .iter()
            .filter_map(|shape| match shape.geom {
                Geom::Rect { x, .. } => Some(x),
                _ => None,
            })
            .collect();
        x_positions.sort_by(f32::total_cmp);
        x_positions.dedup_by(|left, right| (*left - *right).abs() < 0.01);

        assert_eq!(BAR_COUNT, 64);
        assert_eq!(x_positions.len(), BAR_COUNT);
    }

    #[test]
    fn columns_keep_the_cyan_to_magenta_optics() {
        let engine = lively_engine();
        let shapes = scene(&test_ctx(&engine, WIDTH, HEIGHT));
        let segments = main_segments(&shapes);
        let first = segments
            .iter()
            .min_by(|left, right| rect_x(left).total_cmp(&rect_x(right)))
            .unwrap();
        let last = segments
            .iter()
            .max_by(|left, right| rect_x(left).total_cmp(&rect_x(right)))
            .unwrap();
        let Fill::Solid(left) = first.fill;
        let Fill::Solid(right) = last.fill;
        assert!(left.g > left.r && left.b > left.r);
        assert!(right.r > right.g && right.b > right.g);
    }

    #[test]
    fn peak_caps_float_above_every_column() {
        let engine = lively_engine();
        let shapes = scene(&test_ctx(&engine, WIDTH, HEIGHT));
        let caps = shapes
            .iter()
            .filter(|shape| {
                matches!(shape.geom, Geom::Rect { h, .. } if (h - PEAK_CAP_HEIGHT).abs() < 0.01)
            })
            .count();
        assert_eq!(caps, BAR_COUNT);
    }

    #[test]
    fn entering_segment_keeps_the_soft_fade() {
        let mut bars = [0.0; SPECTRUM_BAND_COUNT];
        bars[0] = 1.01 / SEGMENT_COUNT as f32;
        let peaks = [0.0; SPECTRUM_BAND_COUNT];
        let ctx = ModeCtx {
            peaks: &peaks,
            bars: &bars,
            bass_impact: 0.0,
            bass_aura: 0.0,
            accent: (0.2, 0.7, 0.7),
            accent2: (0.7, 0.2, 0.7),
            width: WIDTH,
            height: HEIGHT,
        };
        let shapes = scene(&ctx);
        let first_x = WIDTH * HORIZONTAL_MARGIN;
        let mut first_bar = main_segments(&shapes)
            .into_iter()
            .filter(|shape| (rect_x(shape) - first_x).abs() < 0.01)
            .collect::<Vec<_>>();
        first_bar.sort_by(|left, right| rect_y(left).total_cmp(&rect_y(right)));
        let first = first_bar[0];
        let Fill::Solid(fill) = first.fill;
        assert!(fill.a < 0.03);
    }

    fn rect_x(shape: &Shape) -> f32 {
        match shape.geom {
            Geom::Rect { x, .. } => x,
            _ => f32::NAN,
        }
    }

    fn rect_y(shape: &Shape) -> f32 {
        match shape.geom {
            Geom::Rect { y, .. } => y,
            _ => f32::NAN,
        }
    }
}
