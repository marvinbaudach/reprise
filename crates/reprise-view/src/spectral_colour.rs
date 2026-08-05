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

/// The width, in seconds, of the window the colour curve is averaged over
/// before anything draws it — roughly four bars of music.
///
/// The window is defined in *seconds*, never in bars. A bar-defined window
/// would smooth a narrow window differently from a wide one, so the same track
/// would read differently at two window sizes.
pub const CENTROID_WINDOW_S: f64 = 8.0;

/// Averages the raw centroid curve over a fixed window of time.
///
/// The un-averaged spectral centroid swings from beat to beat: neighbouring
/// support points land a third of the axis apart, which is noise, and noise
/// forms no pattern anyone can read. Averaging over `window_s` (centred on each
/// point) turns it into contiguous fields of eight to thirty seconds — an
/// intro, a verse, a breakdown — which is the thing the colour was for.
///
/// The result is constant per track: compute it once, when the curve arrives,
/// and cache it beside the bar heights. Near the ends the window shrinks to
/// what is available instead of being padded, because padding with zeroes
/// would run the first and last seconds of every track into one end of the
/// axis regardless of what is playing there.
///
/// Returns the curve untouched when the duration is unknown or not positive —
/// a window in seconds needs a timescale, and guessing one would be a
/// different smoothing per track.
#[must_use]
pub fn smooth_centroid_over_seconds(raw: &[u8], duration_s: f64, window_s: f64) -> Vec<u8> {
    let half = half_window_frames(raw.len(), duration_s, window_s);
    if half == 0 {
        return raw.to_vec();
    }
    // Prefix sums: every point averages a window, and recomputing each window
    // is quadratic in the curve length for no gain.
    let mut prefix = Vec::with_capacity(raw.len() + 1);
    let mut running = 0_u32;
    prefix.push(running);
    for value in raw {
        running += u32::from(*value);
        prefix.push(running);
    }
    (0..raw.len())
        .map(|index| {
            let start = index.saturating_sub(half);
            let end = (index + half + 1).min(raw.len());
            let count = (end - start) as u32;
            let sum = prefix[end] - prefix[start];
            // Round half up rather than truncating: a curve that only ever
            // rounds down drifts toward the low end of the axis.
            u8::try_from((sum + count / 2) / count).unwrap_or(u8::MAX)
        })
        .collect()
}

/// Half the averaging window, in curve frames, for a curve of `frames` points
/// spanning `duration_s`. Zero means "do not smooth" — an unknown duration, an
/// empty curve, or a window narrower than a single frame.
fn half_window_frames(frames: usize, duration_s: f64, window_s: f64) -> usize {
    if frames < 2 || !duration_s.is_finite() || duration_s <= 0.0 {
        return 0;
    }
    if !window_s.is_finite() || window_s <= 0.0 {
        return 0;
    }
    let frames_per_second = frames as f64 / duration_s;
    let half = ((window_s / 2.0) * frames_per_second).round();
    if !half.is_finite() || half < 1.0 {
        return 0;
    }
    // A window wider than the track averages the whole curve; anything beyond
    // that is the same answer with more arithmetic.
    (half as usize).min(frames)
}

/// How far apart two section boundaries have to be to count as two.
///
/// Calibrated against a real library rather than picked: at 10 s and a step of
/// 18 the detector found eleven to fifteen boundaries in a three-minute song —
/// a picket fence, not a structure. These values land on five to nine, one
/// every half-minute, which is what a song's sections actually look like.
pub const SECTION_MIN_SPACING_S: f64 = 20.0;
/// How far the smoothed curve has to travel across a boundary, on the 0..255
/// centroid scale, before it reads as one.
pub const SECTION_STEP_THRESHOLD: u8 = 26;
/// Half the span the step is measured across, in seconds.
const SECTION_STEP_SPAN_S: f64 = 2.0;

/// Positions, as 0..1 fractions, where the smoothed colour curve turns over
/// hard enough to read as a new section.
///
/// This is what the single-colour seek bar draws its hairlines at: without the
/// spectral fill there is nothing left that says where the music changes, and
/// a plain accent bar with no marks says less than the old one did.
///
/// Expects the *smoothed* curve — run on the raw one every beat is a boundary.
#[must_use]
pub fn section_boundaries(smoothed: &[u8], duration_s: f64) -> Vec<f64> {
    let span = half_window_frames(smoothed.len(), duration_s, SECTION_STEP_SPAN_S * 2.0);
    if span == 0 || smoothed.len() <= span * 2 {
        return Vec::new();
    }
    let frames = smoothed.len();
    let spacing = ((SECTION_MIN_SPACING_S / duration_s) * frames as f64).round() as usize;
    let spacing = spacing.max(1);
    // Rank by step size, then take greedily: two candidates a second apart are
    // one boundary seen twice, and the stronger one is the better witness.
    let mut candidates: Vec<(u16, usize)> = (span..frames - span)
        .filter_map(|index| {
            let before = i32::from(smoothed[index - span]);
            let after = i32::from(smoothed[index + span]);
            let step = (after - before).unsigned_abs();
            (step >= u32::from(SECTION_STEP_THRESHOLD)).then_some((step as u16, index))
        })
        .collect();
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut accepted: Vec<usize> = Vec::new();
    for (_, index) in candidates {
        if accepted.iter().any(|taken| taken.abs_diff(index) < spacing) {
            continue;
        }
        accepted.push(index);
    }
    accepted.sort_unstable();
    accepted
        .into_iter()
        .map(|index| (index as f64 + 0.5) / frames as f64)
        .collect()
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

    /// A curve of `frames` points for a track of `duration_s`, quiet in the
    /// middle and bright at the end — an intro/breakdown shape in miniature.
    fn stepped_curve(frames: usize) -> Vec<u8> {
        (0..frames)
            .map(|index| if index < frames / 2 { 40 } else { 200 })
            .collect()
    }

    #[test]
    fn smoothing_averages_a_window_of_seconds_not_of_points() {
        // 100 points over 100 s is one point per second, so an 8 s window is
        // four points either side: nine points in all.
        let raw: Vec<u8> = (0..100)
            .map(|index| if index == 50 { 255 } else { 0 })
            .collect();
        let smoothed = smooth_centroid_over_seconds(&raw, 100.0, 8.0);
        for index in 46..=54 {
            assert_eq!(smoothed[index], 28, "index {index}");
        }
        assert_eq!(smoothed[45], 0);
        assert_eq!(smoothed[55], 0);
        // Halving the track's duration halves the number of seconds each point
        // covers, so the same window in seconds now spans twice as many points.
        let denser = smooth_centroid_over_seconds(&raw, 50.0, 8.0);
        assert_eq!(denser[42], 15, "an 8 s window is 17 points at 2 points/s");
        assert_eq!(denser[41], 0);
    }

    #[test]
    fn smoothing_is_the_same_curve_at_any_display_width() {
        // The bug this pins: a window defined in display bars smooths a narrow
        // window differently from a wide one, so dragging the window edge
        // changes how the track reads. Smoothing runs on the stored curve,
        // before any bar count exists, so the same track always yields the
        // same curve — and shaping it to two different bar counts afterwards
        // must agree wherever the two rasters line up.
        let raw = stepped_curve(600);
        let smoothed = smooth_centroid_over_seconds(&raw, 300.0, CENTROID_WINDOW_S);
        let narrow = shape_centroid(&smoothed, 60);
        let wide = shape_centroid(&smoothed, 600);
        for bar in 0..60 {
            let averaged: f32 = wide[bar * 10..(bar + 1) * 10].iter().sum::<f32>() / 10.0;
            assert!(
                (narrow[bar] - averaged).abs() < 1e-6,
                "bar {bar}: {} vs {averaged}",
                narrow[bar]
            );
        }
    }

    #[test]
    fn smoothing_shrinks_its_window_at_the_edges_instead_of_padding() {
        // Padding with zeroes would run the first and last seconds of every
        // track into one end of the axis no matter what plays there.
        let raw = vec![200_u8; 100];
        let smoothed = smooth_centroid_over_seconds(&raw, 100.0, 9.0);
        assert_eq!(smoothed.first(), Some(&200));
        assert_eq!(smoothed.last(), Some(&200));
        assert!(smoothed.iter().all(|value| *value == 200));
    }

    #[test]
    fn smoothing_hands_the_curve_back_when_there_is_no_timescale() {
        let raw = stepped_curve(64);
        for duration in [0.0, -12.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                smooth_centroid_over_seconds(&raw, duration, CENTROID_WINDOW_S),
                raw,
                "duration {duration}"
            );
        }
        assert_eq!(smooth_centroid_over_seconds(&raw, 300.0, 0.0), raw);
        assert!(smooth_centroid_over_seconds(&[], 300.0, CENTROID_WINDOW_S).is_empty());
        // A window narrower than one point cannot average anything.
        assert_eq!(smooth_centroid_over_seconds(&raw, 3600.0, 1.0), raw);
    }

    #[test]
    fn a_window_wider_than_the_track_averages_the_whole_curve() {
        let raw = stepped_curve(50);
        let smoothed = smooth_centroid_over_seconds(&raw, 20.0, 600.0);
        assert!(smoothed.iter().all(|value| *value == 120), "{smoothed:?}");
    }

    #[test]
    fn smoothing_turns_beat_to_beat_swing_into_readable_fields() {
        // The diagnosis in one assertion: alternating support points are a
        // half-axis jump between neighbours, which is what made the bar look
        // like a rainbow. After averaging, neighbours agree.
        let raw: Vec<u8> = (0..600)
            .map(|index| if index % 2 == 0 { 40 } else { 210 })
            .collect();
        let jitter = |curve: &[u8]| {
            curve
                .windows(2)
                .map(|pair| u32::from(pair[0].abs_diff(pair[1])))
                .max()
                .unwrap_or(0)
        };
        assert_eq!(jitter(&raw), 170);
        let smoothed = smooth_centroid_over_seconds(&raw, 300.0, CENTROID_WINDOW_S);
        // An odd window over a two-point alternation leaves one point of the
        // swing behind; what matters is the order of magnitude, and this is a
        // seventeenth of what neighbouring bars used to differ by.
        assert!(
            jitter(&smoothed) * 10 < jitter(&raw),
            "still jittering: {}",
            jitter(&smoothed)
        );
    }

    #[test]
    fn section_boundaries_land_on_the_one_place_the_music_turns() {
        let raw = stepped_curve(600);
        let smoothed = smooth_centroid_over_seconds(&raw, 300.0, CENTROID_WINDOW_S);
        let marks = section_boundaries(&smoothed, 300.0);
        assert_eq!(marks.len(), 1, "{marks:?}");
        assert!((marks[0] - 0.5).abs() < 0.02, "{marks:?}");
    }

    #[test]
    fn a_track_without_structure_gets_no_hairlines() {
        // An ambient piece with no hard transitions is meant to look plain.
        // That is the right answer, not a broken one.
        let smoothed = vec![128_u8; 600];
        assert!(section_boundaries(&smoothed, 300.0).is_empty());
        // Too short to measure a step across, and no timescale at all.
        assert!(section_boundaries(&[1, 2, 3], 300.0).is_empty());
        assert!(section_boundaries(&vec![0, 255, 0, 255], 0.0).is_empty());
    }

    #[test]
    fn a_song_gets_a_handful_of_section_marks_and_not_a_picket_fence() {
        // Calibrated against a real library, and this is the guard on that
        // calibration: at the first settings the detector put eleven to
        // fifteen marks in a three-minute song, which is not a structure
        // anyone reads. A synthetic stand-in with eight genuine turns must
        // come back with about eight, not with one per turn plus their edges.
        let frames = 800;
        let duration_s = 200.0;
        let mut raw = vec![0_u8; frames];
        for (index, value) in raw.iter_mut().enumerate() {
            // Eight blocks of 25 s, alternating high and low.
            *value = if (index * 8 / frames) % 2 == 0 { 60 } else { 200 };
        }
        let smoothed = smooth_centroid_over_seconds(&raw, duration_s, CENTROID_WINDOW_S);
        let marks = section_boundaries(&smoothed, duration_s);
        assert!(
            (5..=8).contains(&marks.len()),
            "expected a song's worth of marks, got {}: {marks:?}",
            marks.len()
        );
        // And never two inside the minimum spacing.
        for pair in marks.windows(2) {
            let apart = (pair[1] - pair[0]) * duration_s;
            assert!(apart >= SECTION_MIN_SPACING_S - 1e-9, "{apart} s apart");
        }
    }

    #[test]
    fn two_turns_a_second_apart_are_one_boundary_seen_twice() {
        // A ramp crosses the threshold at every point along it; without the
        // minimum spacing the bar would grow a picket fence there.
        let mut raw = vec![30_u8; 600];
        for (offset, value) in raw[300..320].iter_mut().enumerate() {
            *value = 30 + (offset as u8) * 11;
        }
        for value in raw[320..].iter_mut() {
            *value = 250;
        }
        let smoothed = smooth_centroid_over_seconds(&raw, 300.0, CENTROID_WINDOW_S);
        let marks = section_boundaries(&smoothed, 300.0);
        assert_eq!(marks.len(), 1, "{marks:?}");
        assert!(marks.iter().all(|mark| (0.0..=1.0).contains(mark)));
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
