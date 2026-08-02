//! Display shaping for cached waveform peaks.

/// Buckets quieter than −50 dB relative to the track's own maximum RMS render
/// as fixed 2 px dots instead of scaled bars. Stored values are normalized to
/// the track max, so the threshold is relative: 10^(−50/20).
const SILENCE_RMS: f32 = 0.003_162_28;
pub const SILENCE_DOT_HEIGHT: f64 = 2.0;
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
pub enum DisplayBar {
    Silence,
    Level(f32),
}

/// Aggregates the stored peaks (sqrt-compressed RMS, see `waveform_peaks.rs`)
/// into `count` buckets in the *linear* RMS domain: undo the sqrt compression,
/// average power over the window, take the root. Returns RMS values in 0..1
/// (relative to the track's own maximum).
pub fn aggregate_rms(raw: &[u8], count: usize) -> Vec<f32> {
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
pub fn smooth_neighbors(values: &[f32]) -> Vec<f32> {
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
pub fn shape_display_peaks(raw: &[u8], count: usize) -> Vec<DisplayBar> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_rms_undoes_the_stored_sqrt_compression() {
        // Stored values are sqrt-compressed: v = sqrt(rms) * 255. A stored 255
        // must aggregate back to rms 1.0, a stored 0 to 0.0.
        let rms = aggregate_rms(&[255, 255, 0, 0], 2);
        assert_eq!(rms.len(), 2);
        assert!((rms[0] - 1.0).abs() < 1e-6);
        assert!(rms[1].abs() < 1e-6);
    }

    #[test]
    fn aggregate_rms_handles_empty_input() {
        assert!(aggregate_rms(&[], 10).is_empty());
        assert!(aggregate_rms(&[128], 0).is_empty());
    }

    #[test]
    fn shape_gives_a_compressed_wall_internal_dynamics() {
        // A "loudness war" track: RMS varies only in a narrow, loud band
        // (a 230-ish verse into a 250-ish chorus). Percentile mapping must
        // spread that band across the full height.
        let mut raw = vec![230u8; 100];
        raw.extend([250u8; 100]);
        let bars = shape_display_peaks(&raw, 100);
        let mut lo = f32::MAX;
        let mut hi = f32::MIN;
        for bar in &bars {
            if let DisplayBar::Level(level) = bar {
                lo = lo.min(*level);
                hi = hi.max(*level);
            }
        }
        assert!(
            hi - lo > 0.5,
            "narrow loud band must be spread out, got lo={lo} hi={hi}"
        );
    }

    #[test]
    fn shape_clips_outliers_above_the_high_percentile() {
        // 96 quiet bars, 4 very loud ones: the loud ones sit above p95 and
        // must clip to the full height (1.0 after gamma).
        let mut raw = vec![100u8; 96];
        raw.extend([255u8; 4]);
        let bars = shape_display_peaks(&raw, 100);
        let last = bars.last().unwrap();
        match last {
            DisplayBar::Level(level) => assert!(*level > 0.95, "outlier level {level}"),
            DisplayBar::Silence => panic!("loud bar classified as silence"),
        }
    }

    #[test]
    fn shape_marks_true_silence_as_dots_not_levels() {
        // Stored 0 (and anything below −50 dB of track max) is silence.
        let mut raw = vec![0u8; 10];
        raw.extend([200u8; 90]);
        let bars = shape_display_peaks(&raw, 100);
        assert_eq!(bars[0], DisplayBar::Silence);
        assert!(matches!(bars[99], DisplayBar::Level(_)));
    }

    #[test]
    fn shape_of_a_perfectly_flat_track_sits_mid_height_not_full() {
        // Degenerate percentiles (p10 == p95): render mid-height, never a
        // full-height wall.
        let raw = vec![200u8; 100];
        let bars = shape_display_peaks(&raw, 50);
        for bar in bars {
            match bar {
                DisplayBar::Level(level) => {
                    assert!((0.05..0.95).contains(&level), "flat level {level}");
                }
                DisplayBar::Silence => panic!("flat loud track is not silence"),
            }
        }
    }

    #[test]
    fn smoothing_averages_neighbors_25_50_25() {
        let smoothed = smooth_neighbors(&[0.0, 1.0, 0.0]);
        // Middle: 0.25*0 + 0.5*1 + 0.25*0 = 0.5; edges clamp to themselves:
        // 0.25*0 + 0.5*0 + 0.25*1 = 0.25.
        assert!((smoothed[1] - 0.5).abs() < 1e-6);
        assert!((smoothed[0] - 0.25).abs() < 1e-6);
        assert!((smoothed[2] - 0.25).abs() < 1e-6);
    }
}
