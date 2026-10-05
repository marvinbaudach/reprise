//! A swiped track change hands the outgoing panel's displayed bars to the
//! incoming one (`current_bands` -> `reset_audio_stream` -> `note_track_changed`
//! -> `adopt_shape`). The displayed bars must then continue into the new
//! track's live analysis instead of pumping the whole spectrum.
//!
//! Every run is deterministic: synthetic PCM, a fake clock, no audio files.

use super::*;

const SAMPLE_RATE_HZ: u32 = 48_000;
/// Samples per display tick: 60 fps, the device's `withFrameNanos` cadence.
const FRAMES_PER_TICK: usize = 800;
const TICK: Duration = Duration::from_nanos(16_666_667);
const WARM_TICKS: usize = 600;
const MEASURED_TICKS: usize = 60;
/// Tolerated frame-mean deviation from the no-seed control, as a fraction of
/// the control's own mean.
const TRACKING_TOLERANCE: f32 = 0.12;
/// No single tick may exceed the control by more than this fraction.
const MAX_JUMP_OVER_CONTROL: f32 = 0.15;
/// How many live frames after a pending seed must stay near the seeded shape
/// (the new track's own audio legitimately starts to move the bars after that).
const PENDING_SEED_TICKS: usize = 3;
const MIN_PENDING_SEED_RATIO: f32 = 0.75;
const MAX_PENDING_SEED_RATIO: f32 = 1.15;
const ENVELOPE_STEP_FRAMES: usize = 4_800;
const KICK_PERIOD_FRAMES: usize = 24_000;
const PARTIAL_HZ: [f32; 9] = [
    110.0, 220.0, 330.0, 523.0, 880.0, 1_400.0, 2_300.0, 4_100.0, 7_200.0,
];

/// Deterministic pseudo-random value in `[0, 1)` for an integer key.
fn unit_hash(key: u64) -> f32 {
    let mut x = key.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03;
    x ^= x >> 29;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 32;
    ((x >> 40) as f32) / (1u64 << 24) as f32
}

/// A sum of sines whose per-partial amplitude follows a seeded random
/// envelope (re-drawn every 100 ms), plus a kick burst twice a second. Two
/// seeds give two different tracks at the same overall loudness.
fn synth_pcm(seed: u64, first_frame: usize, frame_count: usize) -> Vec<u8> {
    let mut pcm = Vec::with_capacity(frame_count * 4);
    for frame in first_frame..first_frame + frame_count {
        let step = (frame / ENVELOPE_STEP_FRAMES) as u64;
        let t = frame as f32 / SAMPLE_RATE_HZ as f32;
        let mut sample = 0.0_f32;
        for (index, hz) in PARTIAL_HZ.iter().enumerate() {
            let envelope = 0.3 + 0.7 * unit_hash(seed * 1_000 + step * 16 + index as u64);
            let phase_offset = unit_hash(seed * 77 + index as u64) * std::f32::consts::TAU;
            sample += envelope * (std::f32::consts::TAU * hz * t + phase_offset).sin();
        }
        let since_kick = (frame % KICK_PERIOD_FRAMES) as f32 / SAMPLE_RATE_HZ as f32;
        sample += 3.0 * (-since_kick * 25.0).exp() * (std::f32::consts::TAU * 55.0 * t).sin();
        let value = (sample / 8.0 * 20_000.0).round().clamp(-32_000.0, 32_000.0) as i16;
        pcm.extend_from_slice(&value.to_le_bytes());
        pcm.extend_from_slice(&value.to_le_bytes());
    }
    pcm
}

fn mean(bands: &[f32]) -> f32 {
    bands.iter().sum::<f32>() / bands.len() as f32
}

struct Playback {
    engine: AndroidVisualEngine,
    clock: Arc<FakeMonotonicClock>,
    next_frame: usize,
}

impl Playback {
    fn new() -> Self {
        let clock = Arc::new(FakeMonotonicClock::default());
        let engine = AndroidVisualEngine::with_clock(clock.clone());
        engine.set_playing(true);
        engine.set_playback_intended(true);
        Self {
            engine,
            clock,
            next_frame: 0,
        }
    }

    /// One display tick: the PCM that arrived since the last tick, then the tick.
    fn tick(&mut self, seed: u64) -> Vec<f32> {
        let pcm = synth_pcm(seed, self.next_frame, FRAMES_PER_TICK);
        self.next_frame += FRAMES_PER_TICK;
        assert!(self
            .engine
            .ingest_pcm_i16(pcm.clone(), pcm.len() as u32, SAMPLE_RATE_HZ, 2));
        self.clock.advance(TICK);
        self.engine.tick();
        self.engine.current_bands()
    }

    fn warm_up(&mut self, seed: u64) {
        for _ in 0..WARM_TICKS {
            self.tick(seed);
        }
    }

    /// The Kotlin swipe: read the shown bars, flush the audio stream, note
    /// the track change, then (optionally) adopt the bars read before.
    fn swipe(&mut self, adopt: bool) {
        let shown = self.engine.current_bands();
        self.engine.reset_audio_stream();
        self.engine.note_track_changed();
        if adopt {
            self.engine.adopt_shape(shown);
        }
    }
}

fn frame_means_after_swipe(adopt: bool) -> Vec<f32> {
    let mut playback = Playback::new();
    playback.warm_up(1);
    playback.swipe(adopt);
    (0..MEASURED_TICKS)
        .map(|_| mean(&playback.tick(2)))
        .collect()
}

#[test]
fn control_run_is_live_and_not_saturated() {
    let control = frame_means_after_swipe(false);

    let level = mean(&control);
    assert!(
        (0.1..0.8).contains(&level),
        "the fixture should draw a moderate, unclipped spectrum: mean={level:.3}"
    );
}

#[test]
fn a_swiped_track_change_does_not_pump_the_spectrum() {
    let control = frame_means_after_swipe(false);
    let seeded = frame_means_after_swipe(true);

    for (index, (control_mean, seeded_mean)) in control.iter().zip(&seeded).enumerate() {
        let deviation = (seeded_mean - control_mean).abs() / control_mean.max(1.0e-3);
        let jump = seeded_mean / control_mean.max(1.0e-3) - 1.0;
        assert!(
            deviation <= TRACKING_TOLERANCE && jump <= MAX_JUMP_OVER_CONTROL,
            "tick {index}: the adopted shape made the spectrum leave the unseeded \
             track change: control={control_mean:.3}, seeded={seeded_mean:.3}"
        );
    }
}

#[test]
fn an_adopted_shape_is_not_followed_by_a_sag_either() {
    let control = frame_means_after_swipe(false);
    let seeded = frame_means_after_swipe(true);

    let control_late = mean(&control[MEASURED_TICKS / 2..]);
    let seeded_late = mean(&seeded[MEASURED_TICKS / 2..]);
    assert!(
        seeded_late >= control_late * (1.0 - TRACKING_TOLERANCE),
        "the spectrum sagged after the adopted shape: control={control_late:.3}, \
         seeded={seeded_late:.3}"
    );
}

/// A bass-heavy shape of the kind a phone shows during a quiet passage: the
/// lowest bands stand tallest, the highs trail off, and nothing comes near the
/// top of the range, so the ceiling a pending boundary estimate applies cannot
/// hide the seed.
fn quiet_shape() -> Vec<f32> {
    (0..SPECTRUM_BAND_COUNT)
        .map(|index| {
            let slope = 0.3 - 0.2 * index as f32 / SPECTRUM_BAND_COUNT as f32;
            slope * (0.85 + 0.15 * unit_hash(500 + index as u64))
        })
        .collect()
}

#[test]
fn a_pending_seed_on_a_fresh_engine_continues_into_the_first_live_frames() {
    // The incoming panel's engine is new: its seed waits for the first PCM
    // block to create the processor, whose smoother has no history yet.
    let shape = quiet_shape();
    let seed_mean = mean(&shape);
    let mut fresh = Playback::new();
    fresh.engine.note_track_changed();
    fresh.engine.adopt_shape(shape);

    let frames: Vec<f32> = (0..PENDING_SEED_TICKS)
        .map(|_| mean(&fresh.tick(2)))
        .collect();

    for (index, frame_mean) in frames.iter().enumerate() {
        let ratio = frame_mean / seed_mean;
        assert!(
            (MIN_PENDING_SEED_RATIO..=MAX_PENDING_SEED_RATIO).contains(&ratio),
            "live frame {index} left the adopted shape: seed mean={seed_mean:.3}, \
             frame means={frames:.3?}"
        );
    }
}
