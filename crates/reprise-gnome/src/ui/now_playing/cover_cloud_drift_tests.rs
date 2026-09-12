use super::*;

#[test]
fn ac_24_every_oil_lamp_parameter_stays_inside_its_declared_range() {
    for blob in all_blobs() {
        for step in 0..=12_000 {
            let drift = drift_at(f64::from(step) * 0.05, blob.drift);
            for (value, range) in drift_components(drift).into_iter().zip(DRIFT_RANGES) {
                assert!(
                    value >= range.0 - 1e-12 && value <= range.1 + 1e-12,
                    "{value} left {range:?} at step {step}"
                );
            }
        }
    }
}

#[test]
fn ac_24_the_oil_lamp_translation_stays_below_the_speed_limit() {
    for blob in all_blobs() {
        let peak_x = analytic_translation_speed_bound(blob.drift.x, DRIFT_X);
        let peak_y = analytic_translation_speed_bound(blob.drift.y, DRIFT_Y);
        assert!(
            peak_x <= DRIFT_SPEED_LIMIT + 1e-9,
            "x peaks at {peak_x:.6} field-fractions/s"
        );
        assert!(
            peak_y <= DRIFT_SPEED_LIMIT + 1e-9,
            "y peaks at {peak_y:.6} field-fractions/s"
        );
    }
}

#[test]
fn ac_24_the_oil_lamp_drift_is_continuous() {
    const STEP_S: f64 = 0.01;
    for blob in all_blobs() {
        let mut previous = normalized_drift(drift_at(0.0, blob.drift));
        let mut total_step = 0.0;
        let mut largest_step: f64 = 0.0;
        let sample_count = 60_000;
        for step in 1..=sample_count {
            let current = normalized_drift(drift_at(f64::from(step) * STEP_S, blob.drift));
            let distance = current
                .into_iter()
                .zip(previous)
                .map(|(current, previous)| (current - previous).abs())
                .fold(0.0, f64::max);
            total_step += distance;
            largest_step = largest_step.max(distance);
            previous = current;
        }
        let mean_step = total_step / f64::from(sample_count);
        assert!(
            largest_step <= mean_step * 3.0,
            "one normalized step ({largest_step:.6}) dwarfs its neighbours ({mean_step:.6} mean)"
        );
    }
}

#[test]
fn ac_24_the_old_eighty_second_pair_period_is_gone() {
    for blob in all_blobs() {
        for elapsed_s in [0.0, 17.0, 43.0, 91.0, 157.0, 239.0] {
            let now = normalized_drift(drift_at(elapsed_s, blob.drift));
            let later = normalized_drift(drift_at(elapsed_s + 80.0, blob.drift));
            let distance = now
                .into_iter()
                .zip(later)
                .map(|(now, later)| (now - later).abs())
                .fold(0.0, f64::max);
            assert!(
                distance >= 0.05,
                "the pose nearly repeated after 80 s at {elapsed_s}: {distance:.6}"
            );
        }
    }
}

#[test]
fn ac_24_each_pose_parameter_reaches_its_extreme_at_a_different_time() {
    for blob in all_blobs() {
        let extremes = extreme_times(blob.drift, 240.0, 0.05);
        for (index, first) in extremes.iter().enumerate() {
            for second in &extremes[index + 1..] {
                assert!(
                    (first - second).abs() >= 2.0,
                    "two parameters turn together at {first:.2} s and {second:.2} s"
                );
            }
        }
    }
}

#[test]
fn ac_24_no_two_clouds_hold_the_same_pose_over_a_long_window() {
    let blobs: Vec<_> = all_blobs().collect();
    for step in 0..=6_000 {
        let elapsed_s = f64::from(step) * 0.1;
        for (index, first) in blobs.iter().enumerate() {
            for second in &blobs[index + 1..] {
                let first = normalized_drift(drift_at(elapsed_s, first.drift));
                let second = normalized_drift(drift_at(elapsed_s, second.drift));
                let distance = first
                    .into_iter()
                    .zip(second)
                    .map(|(first, second)| (first - second).abs())
                    .fold(0.0, f64::max);
                assert!(
                    distance >= 0.005,
                    "two drops coincide at {elapsed_s:.1} s: {distance:.6}"
                );
            }
        }
    }
}

#[test]
fn ac_24_two_clouds_both_approach_and_part_over_a_long_window() {
    let first = &BACK_BLOBS[0];
    let second = &BACK_BLOBS[1];
    let mut previous = drop_distance(0.0, first, second);
    let mut approached = false;
    let mut parted = false;
    for step in 1..=6_000 {
        let current = drop_distance(f64::from(step) * 0.1, first, second);
        approached |= current < previous - 1e-6;
        parted |= current > previous + 1e-6;
        previous = current;
    }
    assert!(approached, "the chosen drops never approach each other");
    assert!(parted, "the chosen drops never part from each other");
}

#[test]
fn ac_24_every_cloud_centre_stays_inside_the_field_at_its_extremes() {
    for blob in all_blobs() {
        assert!(blob.x + DRIFT_X.0 >= 0.0);
        assert!(blob.x + DRIFT_X.1 <= 1.0);
        assert!(blob.y + DRIFT_Y.0 >= 0.0);
        assert!(blob.y + DRIFT_Y.1 <= 1.0);
    }
}

#[test]
fn ac_24_every_parameter_and_cloud_has_its_own_periods_and_phases() {
    let waves: Vec<_> = all_blobs()
        .flat_map(|blob| [blob.drift.x, blob.drift.y, blob.drift.scale])
        .collect();
    let periods: Vec<_> = waves
        .iter()
        .flat_map(|axis| [axis.slow_s, axis.fast_s])
        .collect();
    let phases: Vec<_> = waves
        .iter()
        .flat_map(|axis| [axis.slow_phase, axis.fast_phase])
        .collect();
    assert_all_unique(&periods, "period");
    assert_all_unique(&phases, "phase");
    for blob in all_blobs() {
        let periods = [
            blob.drift.x.slow_s as u64,
            blob.drift.x.fast_s as u64,
            blob.drift.y.slow_s as u64,
            blob.drift.y.fast_s as u64,
            blob.drift.scale.slow_s as u64,
            blob.drift.scale.fast_s as u64,
        ];
        for (index, first) in periods.iter().enumerate() {
            for second in &periods[index + 1..] {
                assert_eq!(greatest_common_divisor(*first, *second), 1);
            }
        }
    }
}

#[test]
fn ac_24_the_declared_drift_ranges_stay_at_the_mockup_amplitude() {
    assert_eq!(DRIFT_X, (-0.20, 0.16));
    assert_eq!(DRIFT_Y, (-0.12, 0.12));
    assert_eq!(DRIFT_SCALE, (1.40, 1.55));
}

#[test]
fn ac_24_each_cloud_raster_covers_the_visible_band_with_a_safe_margin() {
    let clip = (0.0, 0.0, 300.0, f64::from(tokens::NOW_PLAYING_ARTWORK_BAND));
    let field = field(clip.2, f64::from(tokens::NOW_PLAYING_COVER_SIZE));

    for (index, blob) in all_blobs().enumerate() {
        let margins = worst_case_raster_margins(*blob, field, clip);
        println!("cloud {} raster margins L/R/T/B: {margins:?}", index + 1);
        assert!(
            margins
                .into_iter()
                .all(|margin| margin >= MIN_VISIBLE_RASTER_MARGIN_PX),
            "cloud {index} leaves only {margins:?} px around the visible band"
        );
    }
}

const DRIFT_RANGES: [(f64, f64); 3] = [DRIFT_X, DRIFT_Y, DRIFT_SCALE];

fn all_blobs() -> impl Iterator<Item = &'static Blob> {
    BACK_BLOBS.iter().chain(FRONT_BLOBS.iter())
}

fn drift_components(drift: Drift) -> [f64; 3] {
    [drift.x, drift.y, drift.scale]
}

fn normalized_drift(drift: Drift) -> [f64; 3] {
    let components = drift_components(drift);
    std::array::from_fn(|index| {
        let range = DRIFT_RANGES[index];
        (components[index] - range.0) / (range.1 - range.0)
    })
}

pub(super) fn analytic_translation_speed_bound(axis: DriftAxis, range: (f64, f64)) -> f64 {
    let half_range = (range.1 - range.0) / 2.0;
    half_range
        * (SLOW_WAVE_WEIGHT * std::f64::consts::TAU / axis.slow_s
            + FAST_WAVE_WEIGHT * std::f64::consts::TAU / axis.fast_s)
}

fn worst_case_raster_margins(
    blob: Blob,
    field: (f64, f64, f64, f64),
    clip: (f64, f64, f64, f64),
) -> [f64; 4] {
    raster_margins_at_extremes(blob, field, clip)
}

fn extreme_times(profile: DriftProfile, duration_s: f64, step_s: f64) -> [f64; 3] {
    let mut extremes = [f64::NEG_INFINITY; 3];
    let mut times = [0.0; 3];
    for step in 0..=(duration_s / step_s) as u32 {
        let elapsed_s = f64::from(step) * step_s;
        for (index, value) in normalized_drift(drift_at(elapsed_s, profile))
            .into_iter()
            .enumerate()
        {
            if value > extremes[index] {
                extremes[index] = value;
                times[index] = elapsed_s;
            }
        }
    }
    times
}

fn drop_distance(elapsed_s: f64, first: &Blob, second: &Blob) -> f64 {
    let first_drift = drift_at(elapsed_s, first.drift);
    let second_drift = drift_at(elapsed_s, second.drift);
    let dx = first.x + first_drift.x - second.x - second_drift.x;
    let dy = first.y + first_drift.y - second.y - second_drift.y;
    dx.hypot(dy)
}

fn assert_all_unique(values: &[f64], name: &str) {
    for (index, first) in values.iter().enumerate() {
        for second in &values[index + 1..] {
            assert!((first - second).abs() > 1e-9, "shared {name}: {first}");
        }
    }
}

fn greatest_common_divisor(mut first: u64, mut second: u64) -> u64 {
    while second != 0 {
        (first, second) = (second, first % second);
    }
    first
}
