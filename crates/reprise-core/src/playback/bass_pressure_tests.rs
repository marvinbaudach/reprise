use super::bass_pressure::{BassPressureDetector, STEADY_GLOW};

const RATE: u32 = 44_100;

/// A sine of `frequency_hz` whose bass-band RMS lands on `level_dbfs`.
fn sine(frequency_hz: f32, level_dbfs: f32, seconds: f32) -> Vec<f32> {
    let amplitude = std::f32::consts::SQRT_2 * 10.0_f32.powf(level_dbfs / 20.0);
    let count = (RATE as f32 * seconds) as usize;
    (0..count)
        .map(|index| {
            let phase = std::f32::consts::TAU * frequency_hz * index as f32 / RATE as f32;
            amplitude * phase.sin()
        })
        .collect()
}

/// Feeds `samples` in realistic PCM chunks and returns the final reading.
fn observe_all(detector: &mut BassPressureDetector, samples: &[f32]) -> super::BassPressure {
    let mut last = detector.observe(&[]);
    for chunk in samples.chunks(1_024) {
        last = detector.observe(chunk);
    }
    last
}

#[test]
fn a_full_scale_bass_sine_reads_its_true_level() {
    let mut detector = BassPressureDetector::new(RATE);

    let reading = observe_all(&mut detector, &sine(60.0, -3.0, 1.0));

    assert!(
        (reading.level_dbfs + 3.0).abs() < 1.0,
        "a full-scale 60 Hz sine must read about -3 dBFS, got {:.2}",
        reading.level_dbfs
    );
}

#[test]
fn ac_23_quiet_passages_never_ignite_the_glow() {
    let mut detector = BassPressureDetector::new(RATE);

    // A quiet sung passage: the bass band sits around -45 dBFS the whole time.
    let reading = observe_all(&mut detector, &sine(60.0, -45.0, 4.0));

    assert!(
        reading.impact < 0.05,
        "quiet bass must stay dark, got impact {:.3}",
        reading.impact
    );
}

#[test]
fn ac_23_high_frequency_energy_alone_leaves_the_glow_dark() {
    let mut detector = BassPressureDetector::new(RATE);

    // Full-scale vocals/cymbals well above the bass band.
    let reading = observe_all(&mut detector, &sine(4_000.0, -3.0, 4.0));

    assert!(
        reading.impact < 0.05,
        "treble must not drive the bass glow, got impact {:.3}",
        reading.impact
    );
}

#[test]
fn ac_23_steady_loud_bass_keeps_only_the_low_rhythmic_glow() {
    let mut detector = BassPressureDetector::new(RATE);

    // A wall-of-sound track: loud, but without a swell above its own baseline.
    let reading = observe_all(&mut detector, &sine(60.0, -14.0, 6.0));

    assert!(
        (reading.impact - STEADY_GLOW).abs() < 0.05,
        "steady loud bass should rest at the low glow, got impact {:.3}",
        reading.impact
    );
    assert_eq!(
        reading.aura, 0.0,
        "steady loud bass must not reach the breakdown aura"
    );
}

#[test]
fn ac_23_a_bass_drop_over_the_running_baseline_ignites_the_glow() {
    let mut detector = BassPressureDetector::new(RATE);

    // Three seconds of restrained bass establish the baseline, then the drop.
    observe_all(&mut detector, &sine(60.0, -30.0, 3.0));
    let reading = observe_all(&mut detector, &sine(60.0, -10.0, 0.3));

    assert!(
        reading.impact > 0.9,
        "a drop far above the baseline must glow fully, got impact {:.3}",
        reading.impact
    );
}

#[test]
fn ac_23_sustained_breakdown_pressure_escalates_beyond_the_kick_glow() {
    let mut detector = BassPressureDetector::new(RATE);
    observe_all(&mut detector, &sine(60.0, -30.0, 3.0));

    // A single kick is short; a breakdown keeps the pressure up for seconds.
    let kick = observe_all(&mut detector, &sine(60.0, -10.0, 0.1));
    let breakdown = observe_all(&mut detector, &sine(60.0, -10.0, 1.5));

    assert_eq!(kick.aura, 0.0, "one kick must not reach the aura");
    assert!(
        breakdown.aura > 0.3,
        "a sustained breakdown must reach the aura, got {:.3}",
        breakdown.aura
    );
}

#[test]
fn ac_23_the_glow_releases_after_the_impulse_instead_of_flickering() {
    let mut detector = BassPressureDetector::new(RATE);
    observe_all(&mut detector, &sine(60.0, -30.0, 3.0));
    observe_all(&mut detector, &sine(60.0, -10.0, 0.1));

    // 50 ms after the hit the glow still carries; a second later it is gone.
    let shortly_after = observe_all(&mut detector, &vec![0.0; RATE as usize / 20]);
    let much_later = observe_all(&mut detector, &vec![0.0; RATE as usize]);

    assert!(
        shortly_after.impact > 0.3,
        "the glow must carry past the impulse, got impact {:.3}",
        shortly_after.impact
    );
    assert_eq!(much_later.impact, 0.0);
}

#[test]
fn hostile_samples_are_neutralized() {
    let mut detector = BassPressureDetector::new(RATE);

    let reading = observe_all(
        &mut detector,
        &[f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.0e30, -1.0e30].repeat(4_096),
    );

    assert!(reading.level_dbfs.is_finite());
    assert!(reading.baseline_dbfs.is_finite());
    assert!((0.0..=1.0).contains(&reading.impact));
    assert!((0.0..=1.0).contains(&reading.aura));
}

#[test]
fn reset_clears_the_running_baseline() {
    let mut detector = BassPressureDetector::new(RATE);
    observe_all(&mut detector, &sine(60.0, -10.0, 3.0));

    detector.reset();
    let reading = observe_all(&mut detector, &sine(60.0, -45.0, 1.0));

    assert!(
        reading.impact < 0.05,
        "a reset detector must not carry the previous track's pressure, got {:.3}",
        reading.impact
    );
}
