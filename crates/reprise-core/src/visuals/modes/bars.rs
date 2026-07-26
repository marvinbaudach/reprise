//! Bars: twenty finely segmented neon columns inspired by a hardware spectrum
//! analyzer. Spectrum groups shape the individual columns; the shared
//! same-frame beat pulse lifts them together, while slow peak caps preserve a
//! readable frequency silhouette between hits.

use crate::playback::SPECTRUM_BAND_COUNT;
use crate::visuals::color::hsla_to_rgb;

use super::super::engine::ModeCtx;
use super::super::scene::{Fill, Geom, Rgba, Shape};

pub(crate) const BAR_COUNT: usize = 20;
const SEGMENT_COUNT: usize = 16;
const HORIZONTAL_MARGIN: f32 = 0.045;
const BAR_GAP: f32 = 0.008;
const BASELINE: f32 = 0.82;
const MAX_HEIGHT: f32 = 0.68;
const SEGMENT_GAP: f32 = 2.5;
const PEAK_CAP_HEIGHT: f32 = 2.5;
const PEAK_MIN: f32 = 0.04;
const REFLECTION_SEGMENTS: usize = 6;
const BEAT_LIFT_LOW: f32 = 0.88;
const BEAT_LIFT_HIGH: f32 = 0.62;
/// Hits below this captured absolute-energy strength remain frequency-shaped
/// detail instead of lifting the entire analyzer.
const BREAKDOWN_STRENGTH_FLOOR: f32 = 0.25;
/// At this strength a hit receives the full whole-analyzer lift. The value is
/// anchored to the strongest captured bass impact in The Browning's "Wake Up".
const BREAKDOWN_STRENGTH_FULL: f32 = 0.85;
const HUE_START: f32 = 188.0;
const HUE_END: f32 = 315.0;
const ENVELOPE_EASING: f32 = 0.65;
const MAX_RISE_PER_TICK: f32 = 3.0 / SEGMENT_COUNT as f32;
const MAX_FALL_PER_TICK: f32 = 1.0 / SEGMENT_COUNT as f32;
const ENVELOPE_SETTLE_EPSILON: f32 = 0.002;

fn group_value(values: &[f32; SPECTRUM_BAND_COUNT], bar: usize) -> f32 {
    let start = bar * SPECTRUM_BAND_COUNT / BAR_COUNT;
    let end = ((bar + 1) * SPECTRUM_BAND_COUNT / BAR_COUNT).max(start + 1);
    let slice = &values[start..end];
    let peak = slice.iter().copied().fold(0.0_f32, f32::max);
    let mean = slice.iter().sum::<f32>() / slice.len() as f32;
    peak * 0.62 + mean * 0.38
}

fn target_value(bands: &[f32; SPECTRUM_BAND_COUNT], beat: f32, bar: usize) -> f32 {
    let across = bar as f32 / (BAR_COUNT - 1) as f32;
    let beat_lift = BEAT_LIFT_LOW + (BEAT_LIFT_HIGH - BEAT_LIFT_LOW) * across;
    let spectrum = group_value(bands, bar);
    let normalized = ((beat - BREAKDOWN_STRENGTH_FLOOR)
        / (BREAKDOWN_STRENGTH_FULL - BREAKDOWN_STRENGTH_FLOOR))
        .clamp(0.0, 1.0);
    let emphasized = normalized * normalized * (3.0 - 2.0 * normalized);
    let beat = emphasized * beat_lift;
    // A beat lifts the remaining headroom instead of adding a fixed amount.
    // That preserves the spectrum silhouette and avoids hard clipping whole
    // columns to full height on the first frame of a strong hit.
    spectrum + (1.0 - spectrum) * beat
}

pub(crate) struct BarsEnvelope {
    values: [f32; BAR_COUNT],
}

impl BarsEnvelope {
    pub(crate) fn new() -> Self {
        Self {
            values: [0.0; BAR_COUNT],
        }
    }

    pub(crate) fn values(&self) -> &[f32; BAR_COUNT] {
        &self.values
    }

    pub(crate) fn advance(&mut self, bands: &[f32; SPECTRUM_BAND_COUNT], beat: f32) -> bool {
        let mut settled = true;
        for (bar, current) in self.values.iter_mut().enumerate() {
            let target = target_value(bands, beat, bar);
            let delta = target - *current;
            let step = (delta * ENVELOPE_EASING).clamp(-MAX_FALL_PER_TICK, MAX_RISE_PER_TICK);
            *current = (*current + step).clamp(0.0, 1.0);
            settled &= delta.abs() < ENVELOPE_SETTLE_EPSILON;
        }
        settled
    }

    pub(crate) fn reset(&mut self) {
        self.values = [0.0; BAR_COUNT];
    }

    pub(crate) fn snap(&mut self, bands: &[f32; SPECTRUM_BAND_COUNT], beat: f32) {
        self.values = std::array::from_fn(|bar| target_value(bands, beat, bar));
    }
}

fn bar_value(ctx: &ModeCtx, bar: usize) -> f32 {
    ctx.bars[bar]
}

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
    let mut shapes = Vec::with_capacity(BAR_COUNT * (SEGMENT_COUNT + REFLECTION_SEGMENTS + 2) + 1);

    if ctx.beat > 0.01 {
        let (r, g, b) = ctx.accent2;
        shapes.push(Shape {
            geom: Geom::RadialGlow {
                cx: ctx.width / 2.0,
                cy: baseline,
                r: ctx.width * (0.28 + ctx.beat * 0.16),
            },
            fill: Fill::Solid(Rgba {
                r,
                g,
                b,
                a: 0.08 + ctx.beat * 0.18,
            }),
            width: 0.0,
            glow: 0.0,
            dash: None,
        });
    }

    for bar in 0..BAR_COUNT {
        let x = margin + bar as f32 * (bar_width + gap);
        let value = bar_value(ctx, bar);
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
            let partial = (fraction - segment as f32).clamp(0.0, 1.0);
            let transition = smoothstep(partial);
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

        let peak = group_value(ctx.peaks, bar);
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
        let engine = lively_engine();
        let shapes = scene(&test_ctx(&engine, WIDTH, HEIGHT));
        assert!(!shapes.is_empty());
        assert!(Scene {
            shapes: shapes.clone()
        }
        .is_finite_and_sane(WIDTH, HEIGHT));
    }

    #[test]
    fn ac_20_draws_twenty_finely_segmented_columns() {
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
        assert_eq!(x_positions.len(), BAR_COUNT);
        assert!(
            main_segments(&shapes).len() >= BAR_COUNT * 11,
            "a slam must visibly fill most neon segments"
        );
    }

    #[test]
    fn columns_span_cyan_to_magenta() {
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
        assert!(
            left.g > left.r && left.b > left.r,
            "left edge must read cyan"
        );
        assert!(
            right.r > right.g && right.b > right.g,
            "right edge must read magenta"
        );
    }

    #[test]
    fn peak_caps_float_above_the_columns() {
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
    fn ac_20_entering_segment_fades_in_from_nearly_transparent() {
        let fraction = 1.01_f32;
        let mut bars = [0.0_f32; BAR_COUNT];
        bars[0] = fraction / SEGMENT_COUNT as f32;
        let peaks = [0.0_f32; SPECTRUM_BAND_COUNT];
        let ctx = ModeCtx {
            peaks: &peaks,
            bars: &bars,
            beat: 0.0,
            accent: (0.2, 0.7, 0.7),
            accent2: (0.7, 0.2, 0.7),
            width: WIDTH,
            height: HEIGHT,
        };
        let shapes = scene(&ctx);
        let first_x = WIDTH * HORIZONTAL_MARGIN;
        let mut first_bar: Vec<_> = main_segments(&shapes)
            .into_iter()
            .filter(|shape| (rect_x(shape) - first_x).abs() < 0.01)
            .collect();
        first_bar.sort_by(|left, right| rect_y(left).total_cmp(&rect_y(right)));
        let Fill::Solid(top) = first_bar[0].fill;

        assert!(
            top.a < 0.03,
            "a newly entering segment must fade in instead of appearing at visible opacity, alpha={}",
            top.a
        );
    }

    #[test]
    fn ac_20_large_kick_lifts_the_whole_analyzer_decisively() {
        use crate::playback::{SpectrumAnalyzer, SPECTRUM_ANALYSIS_BAND_COUNT};
        use crate::visuals::VisualEngine;

        fn active_segments(engine: &VisualEngine) -> usize {
            let shapes = scene(&test_ctx(engine, WIDTH, HEIGHT));
            main_segments(&shapes).len()
        }

        let wall = [-40.0; SPECTRUM_ANALYSIS_BAND_COUNT];
        let mut analyzer = SpectrumAnalyzer::new();
        let mut engine = VisualEngine::new();
        engine.set_playing(true);
        for _ in 0..120 {
            engine.ingest(&analyzer.ingest(wall));
            engine.tick();
        }
        let before = active_segments(&engine);
        let before_values: [f32; BAR_COUNT] = {
            let ctx = test_ctx(&engine, WIDTH, HEIGHT);
            std::array::from_fn(|bar| bar_value(&ctx, bar))
        };

        let mut kick = wall;
        kick[..8].fill(-2.0);
        let hit = analyzer.ingest(kick);
        assert!(hit.beat().fired);
        engine.ingest(&hit);
        engine.tick();
        let after = active_segments(&engine);
        let ctx = test_ctx(&engine, WIDTH, HEIGHT);
        let after_values: [f32; BAR_COUNT] = std::array::from_fn(|bar| bar_value(&ctx, bar));
        let max_value = after_values.iter().copied().max_by(f32::total_cmp).unwrap();
        let max_rise = before_values
            .iter()
            .zip(after_values)
            .map(|(before, after)| after - before)
            .fold(0.0_f32, f32::max);

        assert!(
            after >= before + BAR_COUNT * 2,
            "a large kick must add at least two visible segments per column immediately: before={before}, after={after}"
        );
        assert!(
            max_rise * SEGMENT_COUNT as f32 <= 3.0 + f32::EPSILON * 8.0,
            "the first kick frame must not teleport a column by more than three segments, added={}",
            max_rise * SEGMENT_COUNT as f32
        );
        assert!(
            max_value < 0.98,
            "the first kick frame must start a fast rise instead of saturating a column instantly, max={max_value}"
        );

        engine.tick();
        engine.tick();
        let fast_rise = active_segments(&engine);
        assert!(
            fast_rise >= before + BAR_COUNT * 4,
            "a large kick must reach at least four additional segments per column within three frames: before={before}, after={fast_rise}"
        );
    }

    #[test]
    fn ac_20_bars_release_without_dropping_multiple_full_segments_per_frame() {
        use crate::playback::{SpectrumAnalyzer, SPECTRUM_ANALYSIS_BAND_COUNT};
        use crate::visuals::VisualEngine;

        fn values(engine: &VisualEngine) -> [f32; BAR_COUNT] {
            let ctx = test_ctx(engine, WIDTH, HEIGHT);
            std::array::from_fn(|bar| bar_value(&ctx, bar))
        }

        let wall = [-40.0; SPECTRUM_ANALYSIS_BAND_COUNT];
        let mut analyzer = SpectrumAnalyzer::new();
        let mut engine = VisualEngine::new();
        engine.set_playing(true);
        for _ in 0..120 {
            engine.ingest(&analyzer.ingest(wall));
            engine.tick();
        }

        let mut kick = wall;
        kick[..8].fill(-2.0);
        engine.ingest(&analyzer.ingest(kick));
        engine.tick();
        let mut previous = values(&engine);
        let max_drop = (0..8).fold(0.0_f32, |largest, _| {
            engine.ingest(&analyzer.ingest(wall));
            engine.tick();
            let current = values(&engine);
            let frame_drop = previous
                .iter()
                .zip(current)
                .map(|(before, after)| before - after)
                .fold(0.0_f32, f32::max);
            previous = current;
            largest.max(frame_drop)
        });

        assert!(
            max_drop * SEGMENT_COUNT as f32 <= 1.0,
            "a release frame must not remove more than one full segment, removed={}",
            max_drop * SEGMENT_COUNT as f32
        );
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
