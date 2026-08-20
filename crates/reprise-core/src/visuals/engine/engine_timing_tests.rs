use std::time::Duration;

use super::*;

fn paused_live_engine() -> VisualEngine {
    let bars = std::array::from_fn(|index| 0.2 + index as f32 * 0.7 / 63.0);
    let mut engine = VisualEngine::new();
    engine.set_playing(true);
    engine.ingest((
        &SpectrumFrame::from_cava_bars(bars),
        Duration::from_micros(16_667),
    ));
    engine.set_has_track(true);
    engine.set_playing(false);
    engine.snap_to_static();
    engine
}

fn repeated(pattern: &[Duration], repeats: usize) -> Vec<Duration> {
    pattern
        .iter()
        .copied()
        .cycle()
        .take(pattern.len() * repeats)
        .collect()
}

#[test]
fn ac_27_elapsed_time_keeps_period_across_frame_cadences() {
    let sequences = [
        ("20 Hz", repeated(&[Duration::from_millis(50)], 120)),
        (
            "15 Hz",
            repeated(
                &[
                    Duration::from_millis(67),
                    Duration::from_millis(67),
                    Duration::from_millis(66),
                ],
                30,
            ),
        ),
        (
            "12 Hz",
            repeated(
                &[
                    Duration::from_millis(83),
                    Duration::from_millis(83),
                    Duration::from_millis(84),
                ],
                24,
            ),
        ),
        (
            "irregular",
            repeated(
                &[
                    Duration::from_millis(17),
                    Duration::from_millis(113),
                    Duration::from_millis(41),
                    Duration::from_millis(79),
                    Duration::from_millis(150),
                    Duration::from_millis(200),
                ],
                10,
            ),
        ),
    ];

    for (label, frames) in sequences {
        assert_eq!(frames.iter().sum::<Duration>(), Duration::from_secs(6));
        let mut engine = paused_live_engine();
        let start = engine.display_bands;

        for elapsed in frames {
            engine.advance_by(elapsed);
        }

        let largest_error = engine
            .display_bands
            .iter()
            .zip(start)
            .map(|(actual, expected)| (actual - expected).abs())
            .fold(0.0, f32::max);
        assert!(
            largest_error < 0.000_01,
            "{label} missed the six-second return by {largest_error}"
        );
    }
}

#[test]
fn ac_27_paused_live_upper_third_stays_above_lower_third() {
    const THIRD: usize = SPECTRUM_BAND_COUNT / 3;
    const MINIMUM_GAP: f32 = 0.09;
    let mut engine = paused_live_engine();

    for tick in 0..IDLE_PERIOD_TICKS as usize {
        engine.tick();
        let lower = engine.display_bands[..THIRD].iter().sum::<f32>() / THIRD as f32;
        let upper = engine.display_bands[SPECTRUM_BAND_COUNT - THIRD..]
            .iter()
            .sum::<f32>()
            / THIRD as f32;
        let gap = upper - lower;
        assert!(
            gap > MINIMUM_GAP,
            "field thirds lost their retained shape at tick {tick}: {gap}"
        );
    }
}
