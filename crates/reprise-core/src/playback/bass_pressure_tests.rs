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

/// Diagnostic probe against real decoded audio (not a contract).
/// `REPRISE_PCM=/path/a.raw:Name,/path/b.raw:Name` — mono f32le at 44.1 kHz.
#[test]
#[ignore = "diagnostic probe; needs REPRISE_PCM, run with --ignored --nocapture"]
fn probe_real_tracks() {
    let Ok(spec) = std::env::var("REPRISE_PCM") else {
        return;
    };
    for entry in spec.split(',') {
        let (path, name) = entry.split_once(':').unwrap_or((entry, entry));
        let Ok(bytes) = std::fs::read(path) else {
            println!("{name}: nicht lesbar");
            continue;
        };
        let samples: Vec<f32> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect();
        let mut detector = BassPressureDetector::new(RATE);
        let (mut ks, mut ps, mut is, mut au) = (vec![], vec![], vec![], vec![]);
        for chunk in samples.chunks(1024) {
            let r = detector.observe(chunk);
            ks.push(r.kick);
            ps.push(r.pressure);
            is.push(r.impact);
            au.push(r.aura);
        }
        let skip = ks.len() / 20;
        let f = |v: &[f32]| {
            let s = &v[skip..];
            let mn = s.iter().cloned().fold(1.0f32, f32::min);
            let mx = s.iter().cloned().fold(0.0f32, f32::max);
            let me = s.iter().sum::<f32>() / s.len() as f32;
            let hi = s.iter().filter(|x| **x > 0.6).count() as f32 / s.len() as f32 * 100.0;
            (mn, mx, me, hi)
        };
        let (kmn, kmx, kme, khi) = f(&ks);
        let (pmn, pmx, pme, _) = f(&ps);
        let (imn, imx, ime, ihi) = f(&is);
        let (_, amx, _, _) = f(&au);
        println!("{name}  ({:.0} s)", samples.len() as f32 / RATE as f32);
        println!("   kick     {kmn:.2}..{kmx:.2}  Mittel {kme:.2}   >0.6 in {khi:.0}% der Zeit");
        println!("   pressure {pmn:.2}..{pmx:.2}  Mittel {pme:.2}");
        println!("   impact   {imn:.2}..{imx:.2}  Mittel {ime:.2}   >0.6 in {ihi:.0}%   aura max {amx:.2}");
    }
}

// --- kick / pressure ------------------------------------------------------
//
// The three idioms the per-beat readings exist for. Each one is a case the
// older `impact` cannot answer once a limiter has squeezed the bass envelope:
// measured against these same patterns at 4 dB of dynamics, `impact` never
// left 0.35–0.43.

/// Builds `reps` cycles of `on_s` at `hit` dBFS followed by `off_s` at `floor`.
fn pattern(freq: f32, hit: f32, on_s: f32, floor: f32, off_s: f32, reps: usize) -> Vec<f32> {
    let mut out = Vec::new();
    for _ in 0..reps {
        out.extend(sine(freq, hit, on_s));
        out.extend(sine(freq, floor, off_s));
    }
    out
}

/// Feeds `samples` window by window and returns every reading.
fn readings(samples: &[f32]) -> Vec<super::BassPressure> {
    let mut detector = BassPressureDetector::new(RATE);
    samples
        .chunks(512)
        .map(|chunk| detector.observe(chunk))
        .collect()
}

#[test]
fn a_limited_four_to_the_floor_still_kicks() {
    // 130 BPM, a 120 ms hit every 460 ms, squeezed to 4 dB of dynamics — the
    // exact shape a modern techno master delivers, and the one `push` sleeps
    // through.
    let all = readings(&pattern(60.0, -10.0, 0.12, -14.0, 0.34, 26));
    let settled = &all[all.len() / 5..];
    let peak = settled.iter().map(|r| r.kick).fold(0.0f32, f32::max);
    let trough = settled.iter().map(|r| r.kick).fold(1.0f32, f32::min);

    assert!(peak > 0.6, "a limited kick must still read as one: {peak}");
    assert!(trough < 0.2, "and must fall back between hits: {trough}");
    // The bed stays lit while it does — that is what carries the brightness.
    let pressure = settled.iter().map(|r| r.pressure).fold(1.0f32, f32::min);
    assert!(
        pressure > 0.6,
        "a loud four-to-the-floor is pressure: {pressure}"
    );
}

#[test]
fn an_808_tail_does_not_swallow_the_next_attack() {
    // A hit, a 600 ms decay, then the next one. A symmetric floor would ride
    // the tail up and leave the following attack no contrast at all — this is
    // the test that justifies the asymmetric floor.
    let mut samples = Vec::new();
    for _ in 0..12 {
        samples.extend(sine(50.0, -9.0, 0.08));
        for step in 0..12 {
            samples.extend(sine(50.0, -9.0 - step as f32 * 1.6, 0.05));
        }
        samples.extend(sine(50.0, -30.0, 0.32));
    }
    let all = readings(&samples);
    // Every attack after the first must still clear the bar.
    let settled = &all[all.len() / 4..];
    let peak = settled.iter().map(|r| r.kick).fold(0.0f32, f32::max);
    assert!(
        peak > 0.5,
        "an 808 attack must survive its own tail: {peak}"
    );
}

#[test]
fn a_held_breakdown_is_pressure_without_a_kick() {
    // 1.5 s of wall after a quieter bar. Both halves are checked at once
    // because that is the point: the light must stay on where the attack ends.
    let mut samples = sine(50.0, -16.0, 2.0);
    samples.extend(sine(50.0, -12.0, 4.0));
    let all = readings(&samples);
    let wall = &all[all.len() * 3 / 4..];

    let pressure = wall.iter().map(|r| r.pressure).fold(0.0f32, f32::max);
    assert!(
        pressure > 0.8,
        "a held wall must read as pressure: {pressure}"
    );
    let kick = wall.iter().map(|r| r.kick).sum::<f32>() / wall.len() as f32;
    assert!(kick < 0.2, "there is no attack left to measure: {kick}");
}

#[test]
fn digital_silence_reads_no_kick_and_no_pressure() {
    let all = readings(&vec![0.0f32; RATE as usize]);
    let reading = all
        .last()
        .copied()
        .expect("silence still produces readings");
    assert_eq!(reading.kick, 0.0);
    assert_eq!(reading.pressure, 0.0);
}

#[test]
fn a_hostile_reading_is_bounded_at_the_frame_boundary() {
    let hostile = super::BassPressure {
        level_dbfs: 0.0,
        baseline_dbfs: 0.0,
        impact: 0.0,
        aura: 0.0,
        kick: f32::NAN,
        pressure: 12.0,
    }
    .sanitized();
    assert_eq!(hostile.kick, 0.0);
    assert_eq!(hostile.pressure, 1.0);
}
