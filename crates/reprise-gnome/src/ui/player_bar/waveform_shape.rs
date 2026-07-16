//! Display shaping for cached waveform peaks.

/// Buckets quieter than −50 dB relative to the track's own maximum RMS render
/// as fixed 2 px dots instead of scaled bars. Stored values are normalized to
/// the track max, so the threshold is relative: 10^(−50/20).
const SILENCE_RMS: f32 = 0.003_162_28;
pub(super) const SILENCE_DOT_HEIGHT: f64 = 2.0;
/// Percentile window for the height mapping: p10 → minimum height, p95 →
/// full height, values above clip. This is what gives a uniformly loud
/// (compressed) track visible internal dynamics.
const PERCENTILE_LOW: f64 = 0.10;
const PERCENTILE_HIGH: f64 = 0.95;
/// Gamma applied after the percentile mapping — pushes mid levels down and
/// spreads the visible contrast between verse and chorus.
const HEIGHT_GAMMA: f32 = 1.6;

/// One display bar: either a true-silence dot or an audible level in 0..1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum DisplayBar {
    Silence,
    Level(f32),
}

/// Aggregates the stored peaks (sqrt-compressed RMS, see `waveform_peaks.rs`)
/// into `count` buckets in the *linear* RMS domain: undo the sqrt compression,
/// average power over the window, take the root. Returns RMS values in 0..1
/// (relative to the track's own maximum).
pub(super) fn aggregate_rms(raw: &[u8], count: usize) -> Vec<f32> {
    if raw.is_empty() || count == 0 {
        return Vec::new();
    }
    (0..count)
        .map(|i| {
            let start = i * raw.len() / count;
            let end = (((i + 1) * raw.len() / count).max(start + 1)).min(raw.len());
            let slice = &raw[start..end];
            let mean_power: f32 = slice
                .iter()
                .map(|&v| {
                    let rms = (f32::from(v) / 255.0).powi(2); // undo sqrt compression
                    rms * rms // power
                })
                .sum::<f32>()
                / slice.len() as f32;
            mean_power.sqrt()
        })
        .collect()
}

/// Nearest-rank percentile over an already sorted slice.
fn percentile(sorted: &[f32], p: f64) -> f32 {
    let last = sorted.len() - 1;
    let rank = ((last as f64) * p).round() as usize;
    sorted[rank.min(last)]
}

/// 3-bucket moving average with 25/50/25 weights; edges clamp to themselves.
/// Applied AFTER the percentile mapping, purely against bar-to-bar flicker.
pub(super) fn smooth_neighbors(values: &[f32]) -> Vec<f32> {
    (0..values.len())
        .map(|i| {
            let prev = values[i.saturating_sub(1)];
            let next = values[(i + 1).min(values.len() - 1)];
            0.25 * prev + 0.5 * values[i] + 0.25 * next
        })
        .collect()
}

/// The full display pipeline: aggregate to `count` RMS buckets, map through
/// the p10..p95 percentile window (giving compressed material internal
/// dynamics), apply the gamma curve, smooth, and mark true silence. The
/// degenerate case (all audible buckets equal) renders at mid height rather
/// than as a full wall.
pub(super) fn shape_display_peaks(raw: &[u8], count: usize) -> Vec<DisplayBar> {
    let rms = aggregate_rms(raw, count);
    if rms.is_empty() {
        return Vec::new();
    }
    let mut audible: Vec<f32> = rms
        .iter()
        .copied()
        .filter(|value| *value >= SILENCE_RMS)
        .collect();
    if audible.is_empty() {
        return vec![DisplayBar::Silence; rms.len()];
    }
    audible.sort_by(f32::total_cmp);
    let low = percentile(&audible, PERCENTILE_LOW);
    let high = percentile(&audible, PERCENTILE_HIGH);
    let span = high - low;

    let shaped: Vec<f32> = rms
        .iter()
        .map(|&value| {
            if value < SILENCE_RMS {
                return 0.0;
            }
            let norm = if span <= f32::EPSILON {
                // Degenerate percentile window (≥ ~85% of buckets identical):
                // the flat mass sits at mid height, anything louder than the
                // window still clips to full height.
                if value > high {
                    1.0
                } else {
                    0.5
                }
            } else {
                ((value - low) / span).clamp(0.0, 1.0)
            };
            norm.powf(HEIGHT_GAMMA)
        })
        .collect();
    let smoothed = smooth_neighbors(&shaped);

    rms.iter()
        .zip(smoothed)
        .map(|(&value, level)| {
            if value < SILENCE_RMS {
                DisplayBar::Silence
            } else {
                DisplayBar::Level(level)
            }
        })
        .collect()
}
