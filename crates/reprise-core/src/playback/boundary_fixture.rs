//! Seeded synthetic music and the measures a stream boundary is judged by.
//!
//! The visualizer has to behave at the moment one song replaces another, on
//! every surface that feeds it, so the acceptance tests of the core, the
//! desktop pipeline and the Android engine share this one yardstick. It is
//! compiled in debug builds only, like the other cross-crate test seams.
//!
//! The music is a kick, a bass note, a melody that changes every eighth note
//! and a hi-hat, rendered from the absolute sample index alone so any start
//! offset yields the same audio. A stationary tone would measure nothing: the
//! pumping this guards against is the whole frame breathing, and a constant
//! signal has no frame-level motion for it to be told apart from.

use std::f64::consts::TAU;

use super::SPECTRUM_BAND_COUNT;

/// One drawn frame.
pub type Frame = [f32; SPECTRUM_BAND_COUNT];

/// Frames per second the metrics assume.
pub const FRAMES_PER_SECOND: usize = 60;
/// Frames after the boundary that are not judged: a bar needs a window of the
/// new audio, so a stream that starts from nothing cannot be at height yet.
pub const SETTLE_FRAMES: usize = 18;
/// Frames judged after the settle span: one second.
pub const JUDGED_FRAMES: usize = FRAMES_PER_SECOND;
/// A bar at or above this is pinned.
const PINNED_LEVEL: f32 = 0.99;
/// A frame with this many pinned bars is a wall.
const WALL_BARS: usize = SPECTRUM_BAND_COUNT / 2;
/// A frame whose tallest bar is below this is empty.
const EMPTY_LEVEL: f32 = 0.05;
/// A bar moving by more than this between frames moved.
const MOVE_EPSILON: f32 = 0.005;
/// Share of the bars that must move one way for the frame to count as the whole
/// spectrum moving together.
const COMOVE_BARS: usize = SPECTRUM_BAND_COUNT * 3 / 4;
/// Half-width, in frames, of the moving average the frame mean is detrended by.
const TREND_HALF_WINDOW: usize = 15;

/// Seeded, stateless synthetic music.
#[derive(Debug, Clone, Copy)]
pub struct SyntheticMusic {
    seed: u64,
    rate_hz: u32,
    gain: f32,
}

impl SyntheticMusic {
    pub fn new(seed: u64, rate_hz: u32, gain: f32) -> Self {
        Self {
            seed,
            rate_hz,
            gain,
        }
    }

    /// The same music `decibels` louder or quieter.
    pub fn louder_by(self, decibels: f32) -> Self {
        Self {
            gain: self.gain * 10.0_f32.powf(decibels / 20.0),
            ..self
        }
    }

    /// The same music at another sample rate.
    pub fn at_rate(self, rate_hz: u32) -> Self {
        Self { rate_hz, ..self }
    }

    pub fn rate_hz(&self) -> u32 {
        self.rate_hz
    }

    /// `count` mono samples starting `start` samples into the song.
    pub fn mono(&self, start: usize, count: usize) -> Vec<f32> {
        (start..start + count).map(|n| self.sample(n)).collect()
    }

    /// `count` interleaved stereo samples of 16-bit little-endian PCM.
    pub fn stereo_i16_bytes(&self, start: usize, count: usize) -> Vec<u8> {
        self.mono(start, count)
            .into_iter()
            .flat_map(|sample| {
                let pcm = (sample.clamp(-1.0, 1.0) * 32_767.0) as i16;
                let [low, high] = pcm.to_le_bytes();
                [low, high, low, high]
            })
            .collect()
    }

    fn sample(&self, n: usize) -> f32 {
        let t = n as f64 / f64::from(self.rate_hz);
        let beat = t % 0.5;
        let kick = (-beat / 0.07).exp() * (TAU * 55.0 * t).sin();
        let bass = 0.5 * (0.6 + 0.4 * (TAU * 1.0 * t).sin()) * (TAU * 82.4 * t).sin();
        let step = (t / 0.25) as u64;
        let note =
            [440.0, 494.0, 587.0, 659.0, 784.0, 988.0][(self.noise(step) * 6.0) as usize % 6];
        let lead = (-(t % 0.25) / 0.12).exp() * (TAU * note * t).sin();
        let hat_envelope = (-((t + 0.125) % 0.25) / 0.02).exp();
        let hat = hat_envelope * (self.noise(n as u64 + 1_000_003) * 2.0 - 1.0);
        let mix = 0.42 * kick + 0.16 * bass + 0.14 * lead + 0.10 * hat;
        (f64::from(self.gain) * mix) as f32
    }

    /// Deterministic noise in `0..1` from an index and the seed.
    fn noise(&self, index: u64) -> f64 {
        let mut x = index
            .wrapping_add(self.seed)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15);
        x ^= x >> 32;
        x = x.wrapping_mul(0xD6E8_FEB8_6659_FD93);
        x ^= x >> 32;
        (x >> 11) as f64 / (1_u64 << 53) as f64
    }
}

/// What a run of frames looks like to a viewer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measure {
    /// Detrended frame-mean RMS over the mean level: how much the whole
    /// frame breathes.
    pub depth: f32,
    /// Share of frame steps in which most bars moved the same way.
    pub comove: f32,
    /// Frames with at least one pinned bar.
    pub pinned_frames: usize,
    /// Frames with a wall of pinned bars.
    pub wall_frames: usize,
    /// Frames with no visible bar.
    pub empty_frames: usize,
    /// Mean of the frame means.
    pub level: f32,
}

pub fn frame_mean(frame: &Frame) -> f32 {
    frame.iter().sum::<f32>() / frame.len() as f32
}

impl Measure {
    pub fn of(frames: &[Frame]) -> Self {
        let means: Vec<f32> = frames.iter().map(frame_mean).collect();
        let level = means.iter().sum::<f32>() / means.len() as f32;
        let squares: f32 = (0..means.len())
            .map(|index| {
                let low = index.saturating_sub(TREND_HALF_WINDOW);
                let high = (index + TREND_HALF_WINDOW + 1).min(means.len());
                let trend = means[low..high].iter().sum::<f32>() / (high - low) as f32;
                (means[index] - trend).powi(2)
            })
            .sum();
        let comoving = frames
            .windows(2)
            .filter(|pair| {
                let (up, down) =
                    pair[1]
                        .iter()
                        .zip(&pair[0])
                        .fold((0, 0), |(up, down), (now, before)| {
                            let step = now - before;
                            (
                                up + usize::from(step > MOVE_EPSILON),
                                down + usize::from(step < -MOVE_EPSILON),
                            )
                        });
                up >= COMOVE_BARS || down >= COMOVE_BARS
            })
            .count();
        let pinned = |frame: &&Frame| frame.iter().any(|bar| *bar >= PINNED_LEVEL);
        let wall =
            |frame: &&Frame| frame.iter().filter(|bar| **bar >= PINNED_LEVEL).count() >= WALL_BARS;
        let empty = |frame: &&Frame| frame.iter().all(|bar| *bar < EMPTY_LEVEL);
        Self {
            depth: (squares / means.len() as f32).sqrt() / level.max(1.0e-6),
            comove: comoving as f32 / (frames.len() - 1) as f32,
            pinned_frames: frames.iter().filter(pinned).count(),
            wall_frames: frames.iter().filter(wall).count(),
            empty_frames: frames.iter().filter(empty).count(),
            level,
        }
    }
}

/// The widest the drawn level may stray from the settled reference once the
/// boundary has had a window of audio, as a multiple.
pub const LEVEL_BAND: (f32, f32) = (0.7, 1.4);
/// The widest a tenth of a second of drawn level may stray from the reference's,
/// as a multiple. Looser than the whole-second band only for the noise of a
/// handful of frames; a swell or a dim stretch that long still falls outside.
pub const BLOCK_BAND: (f32, f32) = (0.75, 1.25);
/// Frames per block of [`BLOCK_BAND`].
const BLOCK_FRAMES: usize = 6;
/// How far the whole frame may breathe more than the reference does.
const DEPTH_SLACK: f32 = 0.05;
/// How much more often the whole spectrum may move together than the
/// reference's.
const COMOVE_SLACK: f32 = 0.05;
/// How many more frames than the reference may touch the ceiling.
const PINNED_SLACK: usize = 8;

/// What a boundary run is judged against: the frames the same audio draws on
/// a visualizer that has been running it for long enough to have settled.
pub fn judge_boundary(label: &str, run: &[Frame], reference: &[Frame], expect_no_gap: bool) {
    let end = SETTLE_FRAMES + JUDGED_FRAMES;
    assert!(
        run.len() >= end && reference.len() >= end,
        "{label}: need {end} frames, got {} and {}",
        run.len(),
        reference.len()
    );
    let first_second = Measure::of(&run[..FRAMES_PER_SECOND]);
    let reference_first_second = Measure::of(&reference[..FRAMES_PER_SECOND]);
    assert_eq!(
        first_second.wall_frames, reference_first_second.wall_frames,
        "{label}: a wall of pinned bars in the first second: {first_second:?}"
    );
    assert!(
        first_second.pinned_frames <= reference_first_second.pinned_frames + PINNED_SLACK,
        "{label}: pinned frames {} against the reference's {}",
        first_second.pinned_frames,
        reference_first_second.pinned_frames
    );
    if expect_no_gap {
        assert_eq!(
            first_second.empty_frames, reference_first_second.empty_frames,
            "{label}: the bars collapsed to nothing at the boundary"
        );
    }
    let judged = Measure::of(&run[SETTLE_FRAMES..end]);
    let settled = Measure::of(&reference[SETTLE_FRAMES..end]);
    assert!(
        judged.depth <= settled.depth + DEPTH_SLACK,
        "{label}: the frame breathes {:.3} deep against the reference's {:.3}",
        judged.depth,
        settled.depth
    );
    assert!(
        judged.comove <= settled.comove + COMOVE_SLACK,
        "{label}: the spectrum moves as one {:.2} of the time against the reference's {:.2}",
        judged.comove,
        settled.comove
    );
    let ratio = judged.level / settled.level;
    assert!(
        (LEVEL_BAND.0..=LEVEL_BAND.1).contains(&ratio),
        "{label}: drawn level is {ratio:.2} times the reference's"
    );
    for start in (SETTLE_FRAMES..end).step_by(BLOCK_FRAMES) {
        let level = |frames: &[Frame]| {
            frames[start..start + BLOCK_FRAMES]
                .iter()
                .map(frame_mean)
                .sum::<f32>()
        };
        let block_ratio = level(run) / level(reference);
        assert!(
            (BLOCK_BAND.0..=BLOCK_BAND.1).contains(&block_ratio),
            "{label}: frames {start}..{} are drawn at {block_ratio:.2} times the reference's",
            start + BLOCK_FRAMES
        );
    }
}

/// How far below the reference the shape on screen may dip while a boundary
/// inside a song it already shows settles, as a multiple.
const DIP_FLOOR: f32 = 0.9;

/// A boundary that continues what the viewer is watching (a seek, or a swipe to
/// the same song) must not shrink the frame while the new window fills: every
/// frame of the settle span keeps within [`DIP_FLOOR`] of the reference.
pub fn assert_no_dip(label: &str, run: &[Frame], reference: &[Frame]) {
    for (index, (frame, reference)) in run.iter().zip(reference).take(SETTLE_FRAMES).enumerate() {
        let ratio = frame_mean(frame) / frame_mean(reference).max(1.0e-6);
        assert!(
            ratio >= DIP_FLOOR,
            "{label}: frame {index} dips to {ratio:.2} times the reference's"
        );
    }
}
