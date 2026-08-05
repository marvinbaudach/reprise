//! Spectral-position shaping and the fixed coral-to-teal brand axis.

use std::f64::consts::TAU;

use crate::colour::{oklab_to_srgb, srgb_to_oklab};

pub const CORAL: (u8, u8, u8) = (255, 111, 94);
pub const TEAL: (u8, u8, u8) = (79, 219, 212);

/// Maps the normalized absolute frequency position to the long, falling-hue
/// OKLCH route from coral through magenta and blue to teal.
pub fn spectral_colour(t: f64) -> (f64, f64, f64) {
    // `f64::clamp` passes NaN through unchanged, so clamping alone would carry
    // it all the way into the returned channels and paint the playhead with
    // NaN. This crate is toolkit-neutral and every frontend reaches these
    // functions directly, so the guard belongs here rather than at one caller.
    // Only NaN is redirected: an infinite position still clamps onto an end of
    // the axis, while NaN has no place on it at all, and the middle is the one
    // answer that claims nothing.
    let t = if t.is_nan() { 0.5 } else { t.clamp(0.0, 1.0) };
    let normalized = |colour: (u8, u8, u8)| {
        (
            f64::from(colour.0) / 255.0,
            f64::from(colour.1) / 255.0,
            f64::from(colour.2) / 255.0,
        )
    };
    let start = normalized(CORAL);
    let end = normalized(TEAL);
    let (start_l, start_a, start_b) = srgb_to_oklab(start.0, start.1, start.2);
    let (end_l, end_a, end_b) = srgb_to_oklab(end.0, end.1, end.2);
    let start_c = start_a.hypot(start_b);
    let end_c = end_a.hypot(end_b);
    let start_h = start_b.atan2(start_a);
    let mut end_h = end_b.atan2(end_a);
    while end_h >= start_h {
        end_h -= TAU;
    }
    let l = start_l + (end_l - start_l) * t;
    let c = start_c + (end_c - start_c) * t;
    let h = start_h + (end_h - start_h) * t;
    oklab_to_srgb(l, c * h.cos(), c * h.sin())
}

/// Averages centroid support points into display-bar windows.
pub fn shape_centroid(raw: &[u8], count: usize) -> Vec<f32> {
    if raw.is_empty() || count == 0 {
        return Vec::new();
    }
    (0..count)
        .map(|index| {
            let start = index * raw.len() / count;
            let end = (((index + 1) * raw.len() / count).max(start + 1)).min(raw.len());
            raw[start..end]
                .iter()
                .map(|value| f32::from(*value))
                .sum::<f32>()
                / (end - start) as f32
                / 255.0
        })
        .collect()
}

/// Samples the raw support curve at a normalized position with linear
/// interpolation, independent of the number of display bars.
pub fn centroid_at(raw: &[u8], fraction: f64) -> f64 {
    match raw.len() {
        0 => 0.5,
        1 => f64::from(raw[0]) / 255.0,
        len => {
            // See `spectral_colour`: a NaN fraction survives `clamp`, and
            // `NaN as usize` then saturates to 0 while `amount` stays NaN, so
            // the interpolation would return NaN instead of a support point.
            // An infinite fraction needs no guard — `clamp` handles it.
            let fraction = if fraction.is_nan() { 0.0 } else { fraction };
            let index = fraction.clamp(0.0, 1.0) * (len - 1) as f64;
            let lower = index.floor() as usize;
            let upper = (lower + 1).min(len - 1);
            let amount = index - lower as f64;
            let lower_value = f64::from(raw[lower]);
            let upper_value = f64::from(raw[upper]);
            (lower_value + (upper_value - lower_value) * amount) / 255.0
        }
    }
}

/// Exact exponential smoothing with time constant `tau_s`.
pub fn smooth_towards(
    current: (f64, f64, f64),
    target: (f64, f64, f64),
    dt_s: f64,
    tau_s: f64,
) -> (f64, f64, f64) {
    if dt_s <= 0.0 {
        return current;
    }
    let alpha = if tau_s <= 0.0 {
        1.0
    } else {
        1.0 - (-dt_s / tau_s).exp()
    };
    (
        current.0 + (target.0 - current.0) * alpha,
        current.1 + (target.1 - current.1) * alpha,
        current.2 + (target.2 - current.2) * alpha,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHANNEL_TOLERANCE: f64 = 1.0 / 255.0 + 1e-9;

    fn assert_rgb_close(actual: (f64, f64, f64), expected: (u8, u8, u8)) {
        for (actual, expected) in [
            (actual.0, expected.0),
            (actual.1, expected.1),
            (actual.2, expected.2),
        ] {
            assert!((actual - f64::from(expected) / 255.0).abs() <= CHANNEL_TOLERANCE);
        }
    }

    #[test]
    fn spectral_colour_reaches_the_brand_endpoints() {
        assert_rgb_close(spectral_colour(0.0), CORAL);
        assert_rgb_close(spectral_colour(1.0), TEAL);
        assert_rgb_close(spectral_colour(-1.0), CORAL);
        assert_rgb_close(spectral_colour(2.0), TEAL);
    }

    #[test]
    fn spectral_colour_midpoint_stays_saturated_on_the_long_hue_path() {
        let (r, g, b) = spectral_colour(0.5);
        let (_, a, b) = crate::colour::srgb_to_oklab(r, g, b);
        assert!((a * a + b * b).sqrt() > 0.10);
    }

    #[test]
    fn shape_centroid_averages_source_windows() {
        assert_eq!(
            shape_centroid(&[0, 64, 128, 255], 2),
            vec![32.0 / 255.0, 191.5 / 255.0]
        );
        assert!(shape_centroid(&[], 2).is_empty());
        assert!(shape_centroid(&[1], 0).is_empty());
    }

    #[test]
    fn centroid_at_interpolates_between_support_points() {
        assert!((centroid_at(&[0, 255], 0.5) - 0.5).abs() < 1e-12);
        assert!((centroid_at(&[0, 64, 255], 0.25) - 32.0 / 255.0).abs() < 1e-12);
    }

    #[test]
    fn centroid_at_handles_edges_singletons_and_empty_curves() {
        assert_eq!(centroid_at(&[12, 200], 0.0), 12.0 / 255.0);
        assert_eq!(centroid_at(&[12, 200], 1.0), 200.0 / 255.0);
        assert_eq!(centroid_at(&[77], 0.7), 77.0 / 255.0);
        assert_eq!(centroid_at(&[], 0.3), 0.5);
        assert_eq!(centroid_at(&[12, 200], -1.0), 12.0 / 255.0);
        assert_eq!(centroid_at(&[12, 200], 2.0), 200.0 / 255.0);
    }

    #[test]
    fn non_finite_positions_never_leak_into_the_result() {
        // `f64::clamp` returns NaN for NaN, so both entry points guard
        // explicitly. Without that, the playhead is painted with NaN channels.
        for fraction in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let sampled = centroid_at(&[12, 200], fraction);
            assert!(
                sampled.is_finite(),
                "centroid_at({fraction}) was not finite"
            );
            assert!((0.0..=1.0).contains(&sampled));

            let (r, g, b) = spectral_colour(fraction);
            assert!(
                r.is_finite() && g.is_finite() && b.is_finite(),
                "spectral_colour({fraction}) was not finite"
            );
        }
        // An infinite position still clamps onto the axis rather than
        // collapsing to the middle: only NaN has no place on it.
        assert_eq!(centroid_at(&[12, 200], f64::INFINITY), 200.0 / 255.0);
        assert_eq!(centroid_at(&[12, 200], f64::NEG_INFINITY), 12.0 / 255.0);
        assert_eq!(centroid_at(&[12, 200], f64::NAN), 12.0 / 255.0);
    }

    #[test]
    fn smoothing_covers_one_exponential_time_constant_at_dt_equal_tau() {
        let result = smooth_towards((0.0, 0.0, 0.0), (1.0, 0.5, 0.25), 0.120, 0.120);
        let expected = 1.0 - (-1.0_f64).exp();
        assert!((result.0 - expected).abs() < 1e-12);
        assert!((result.1 - expected * 0.5).abs() < 1e-12);
        assert!((result.2 - expected * 0.25).abs() < 1e-12);
        assert_eq!(
            smooth_towards((0.2, 0.3, 0.4), (1.0, 1.0, 1.0), 0.0, 0.120),
            (0.2, 0.3, 0.4)
        );
    }

    #[test]
    fn smoothing_converges_monotonically_without_overshoot() {
        let target = (0.9, 0.8, 0.7);
        let mut value = (0.1, 0.2, 0.3);
        for _ in 0..100 {
            let next = smooth_towards(value, target, 1.0 / 60.0, 0.120);
            assert!(next.0 >= value.0 && next.0 <= target.0);
            assert!(next.1 >= value.1 && next.1 <= target.1);
            assert!(next.2 >= value.2 && next.2 <= target.2);
            value = next;
        }
        assert!((value.0 - target.0).abs() < 1e-5);
    }
}
