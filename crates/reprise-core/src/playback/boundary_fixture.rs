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
/// Where inside a beat a boundary may land, in seconds past a whole second.
/// The synthetic song's kick falls every half second, so a boundary at a whole
/// second always opens on a kick; these offsets open on the tail of one, on
/// nothing, and just before the next.
pub const BOUNDARY_OFFSETS_SECONDS: [f32; 4] = [0.0, 0.13, 0.29, 0.41];
/// Whole seconds a reference engine has to play a song, from a new processor,
/// before the braking span of a boundary is behind it.
const SETTLE_MARGIN_SECONDS: usize = 1;

/// How long an engine at `rate_hz` has to play a song to be settled on it: the
/// braking span of its first boundary, rounded up to whole seconds, and a
/// margin.
pub fn settled_seconds(rate_hz: u32) -> usize {
    super::cava::braking_span_seconds(rate_hz).ceil() as usize + SETTLE_MARGIN_SECONDS
}

/// A bar at or above this is pinned.
const PINNED_LEVEL: f32 = 0.99;
/// A frame with this many pinned bars is a wall.
const WALL_BARS: usize = SPECTRUM_BAND_COUNT / 2;
/// A frame with this many pinned bars is crowded: a handful of bars at the top
/// is a loud song, this many in a row is the frame stuck against the ceiling.
const CROWDED_BARS: usize = 8;
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
    /// How the song opens, and the sample it opens at.
    opening: Option<(Opening, usize)>,
}

impl SyntheticMusic {
    pub fn new(seed: u64, rate_hz: u32, gain: f32) -> Self {
        Self {
            seed,
            rate_hz,
            gain,
            opening: None,
        }
    }

    /// The same music, opening at sample `start` as `opening` says: quieter or
    /// rising, then at its own level. Before `start` it plays at its own level.
    pub fn opened_by(self, opening: Opening, start: usize) -> Self {
        Self {
            opening: Some((opening, start)),
            ..self
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
        let opening = self.opening.map_or(1.0, |(opening, start)| {
            n.checked_sub(start).map_or(1.0, |since| {
                opening.gain(since as f32 / self.rate_hz as f32)
            })
        });
        (f64::from(self.gain) * mix) as f32 * opening
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

/// How far down a fade on a decibel ramp starts.
const FADE_FLOOR_DB: f32 = 60.0;

/// How a song opens, as the gain applied to it from its first sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Opening {
    /// This many seconds `db` below the song, then the song at once.
    Intro { seconds: usize, db: f32 },
    /// From digital silence to the song, linear in amplitude.
    LinearFade { seconds: usize },
    /// From 60 dB down to the song, linear in decibels.
    DbFade { seconds: usize },
}

impl Opening {
    pub fn seconds(self) -> usize {
        match self {
            Self::Intro { seconds, .. }
            | Self::LinearFade { seconds }
            | Self::DbFade { seconds } => seconds,
        }
    }

    /// The gain `t` seconds into the song.
    pub fn gain(self, t: f32) -> f32 {
        let seconds = self.seconds() as f32;
        match self {
            Self::Intro { db, .. } if t < seconds => 10.0_f32.powf(-db / 20.0),
            Self::LinearFade { .. } if t < seconds => t / seconds,
            Self::DbFade { .. } if t < seconds => {
                10.0_f32.powf(-FADE_FLOOR_DB * (1.0 - t / seconds) / 20.0)
            }
            _ => 1.0,
        }
    }
}

/// The most consecutive crowded frames a stretch may have.
pub const LONGEST_CROWDED_RUN: usize = 25;

/// What a stretch of frames got wrong about pinned bars, if anything: a wall,
/// a long run of crowded frames, or more frames with a pinned bar than
/// `allowed`.
pub fn pinning_complaints(label: &str, measured: Measure, allowed: usize) -> Vec<String> {
    let mut complaints = Vec::new();
    if measured.wall_frames > 0 {
        complaints.push(format!(
            "{label}: {} frames with half the bars pinned",
            measured.wall_frames
        ));
    }
    if measured.longest_crowded_run > LONGEST_CROWDED_RUN {
        complaints.push(format!(
            "{label}: {} frames in a row with eight or more bars pinned",
            measured.longest_crowded_run
        ));
    }
    if measured.pinned_frames > allowed {
        complaints.push(format!(
            "{label}: {} frames pinned, at most {allowed} allowed",
            measured.pinned_frames
        ));
    }
    complaints
}

/// The least a quiet intro's body may be drawn at, as a share of the settled
/// level, averaged over the three seconds after the body arrives. The brakes
/// that catch the body leave it a little dim while the creep climbs back; the
/// three surfaces measure 0.88 to 1.01 on synthetic music, so a gain braked
/// further than a rise needs would show.
pub const INTRO_LEVEL_FLOOR: f32 = 0.85;
/// The same for a fade-in, averaged over its first ten seconds. A fade is
/// followed by a gain that adapts upward, so it reads at or above the settled
/// engine's level (1.04 to 1.56 measured), which draws it small until it ends.
pub const FADE_LEVEL_FLOOR: f32 = 0.95;

/// What a stretch drew too dim, if anything: its mean level against the level
/// the same audio draws on an engine settled on the song, which is what the
/// viewer would have seen without the boundary. Braking a quiet start's gain
/// down must not leave the rest of the song under `floor` times that.
pub fn dimming_complaints(
    label: &str,
    measured: Measure,
    reference: Measure,
    floor: f32,
) -> Vec<String> {
    let ratio = measured.level / reference.level;
    if ratio >= floor {
        return Vec::new();
    }
    vec![format!(
        "{label}: drawn at {ratio:.2} times the settled level, at least {floor} wanted"
    )]
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
    /// The most consecutive crowded frames (at least eight pinned bars).
    pub longest_crowded_run: usize,
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
        let mut crowded_run = 0;
        let mut longest_crowded_run = 0;
        for frame in frames {
            let crowded = frame.iter().filter(|bar| **bar >= PINNED_LEVEL).count() >= CROWDED_BARS;
            crowded_run = if crowded { crowded_run + 1 } else { 0 };
            longest_crowded_run = longest_crowded_run.max(crowded_run);
        }
        Self {
            depth: (squares / means.len() as f32).sqrt() / level.max(1.0e-6),
            comove: comoving as f32 / (frames.len() - 1) as f32,
            pinned_frames: frames.iter().filter(pinned).count(),
            wall_frames: frames.iter().filter(wall).count(),
            longest_crowded_run,
            empty_frames: frames.iter().filter(empty).count(),
            level,
        }
    }
}

/// The widest the drawn level may stray from the settled reference once the
/// boundary has had a window of audio, as a multiple.
pub const LEVEL_BAND: (f32, f32) = (0.7, 1.4);
/// Frames per block of a block band.
const BLOCK_FRAMES: usize = 6;

/// What kind of boundary a run crossed, which decides how closely it must
/// follow the settled reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Boundary {
    /// Nothing was on screen: a first start, or the visualizer switched on.
    /// The bars rise from zero as the first window fills, and what the first
    /// window shows of a song with sparse hits decides the level it is drawn
    /// at until the next hit.
    Fresh,
    /// A shape was on screen and the song is another, at another level.
    Different,
    /// A shape was on screen and the song carries on at its level: a seek, or a
    /// swipe to a song of the same loudness.
    Continuing,
}

/// How far a run may stray from the reference.
struct Limits {
    /// Frame-mean RMS over the level, over the first second, above the reference's.
    first_second_depth: f32,
    /// Share of frame steps moving the whole spectrum, over the first second.
    first_second_comove: f32,
    /// The same two over the second after the settle span.
    settled_depth: f32,
    settled_comove: f32,
    /// The largest step of the frame mean between two frames of the settle span,
    /// as a share of the reference's level. A ramp that rises to the level is a
    /// handful of small steps; a swell that ends in a cliff is one large one.
    settle_step: f32,
    /// Frames more than the reference that touch full height in the first second.
    pinned: usize,
    /// The widest a tenth of a second of drawn level may stray from the
    /// reference's, as a multiple.
    block_band: (f32, f32),
    /// Whether the bars may be empty at the boundary.
    may_start_empty: bool,
}

impl Boundary {
    fn limits(self) -> Limits {
        match self {
            Self::Fresh => Limits {
                first_second_depth: 0.2,
                first_second_comove: 0.18,
                settled_depth: 0.08,
                settled_comove: 0.08,
                settle_step: 0.5,
                pinned: 20,
                block_band: (0.7, 2.3),
                may_start_empty: true,
            },
            Self::Different => Limits {
                first_second_depth: 0.24,
                first_second_comove: 0.2,
                settled_depth: 0.12,
                settled_comove: 0.12,
                settle_step: 0.4,
                pinned: 24,
                block_band: (0.55, 2.0),
                may_start_empty: false,
            },
            Self::Continuing => Limits {
                first_second_depth: 0.04,
                first_second_comove: 0.03,
                settled_depth: 0.02,
                settled_comove: 0.03,
                settle_step: 0.15,
                pinned: 10,
                block_band: (0.85, 1.2),
                may_start_empty: false,
            },
        }
    }
}

/// What a boundary run is judged against: the frames the same audio draws on
/// a visualizer that has been running it for long enough to have settled.
pub fn judge_boundary(label: &str, run: &[Frame], reference: &[Frame], kind: Boundary) {
    let limits = kind.limits();
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
        first_second.pinned_frames <= reference_first_second.pinned_frames + limits.pinned,
        "{label}: pinned frames {} against the reference's {}",
        first_second.pinned_frames,
        reference_first_second.pinned_frames
    );
    assert!(
        first_second.depth <= reference_first_second.depth + limits.first_second_depth,
        "{label}: over the first second the frame breathes {:.3} deep against the \
         reference's {:.3}",
        first_second.depth,
        reference_first_second.depth
    );
    assert!(
        first_second.comove <= reference_first_second.comove + limits.first_second_comove,
        "{label}: over the first second the spectrum moves as one {:.2} of the time \
         against the reference's {:.2}",
        first_second.comove,
        reference_first_second.comove
    );
    judge_settle_span(label, run, reference, limits.settle_step);
    if !limits.may_start_empty {
        assert_eq!(
            first_second.empty_frames, reference_first_second.empty_frames,
            "{label}: the bars collapsed to nothing at the boundary"
        );
    }
    let judged = Measure::of(&run[SETTLE_FRAMES..end]);
    let settled = Measure::of(&reference[SETTLE_FRAMES..end]);
    assert!(
        judged.depth <= settled.depth + limits.settled_depth,
        "{label}: the frame breathes {:.3} deep against the reference's {:.3}",
        judged.depth,
        settled.depth
    );
    assert!(
        judged.comove <= settled.comove + limits.settled_comove,
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
            (limits.block_band.0..=limits.block_band.1).contains(&block_ratio),
            "{label}: frames {start}..{} are drawn at {block_ratio:.2} times the reference's",
            start + BLOCK_FRAMES
        );
    }
}

/// How far below the reference the shape on screen may dip while a boundary
/// inside a song it already shows settles, as a multiple.
const DIP_FLOOR: f32 = 0.8;

/// A boundary that continues what the viewer is watching (a seek, or a swipe to
/// the same song) must not shrink the frame while the new window fills: every
/// frame of the settle span keeps within `DIP_FLOOR` of the reference.
pub fn assert_no_dip(label: &str, run: &[Frame], reference: &[Frame]) {
    for (index, (frame, reference)) in run.iter().zip(reference).take(SETTLE_FRAMES).enumerate() {
        let ratio = frame_mean(frame) / frame_mean(reference).max(1.0e-6);
        assert!(
            ratio >= DIP_FLOOR,
            "{label}: frame {index} dips to {ratio:.2} times the reference's"
        );
    }
}

/// The settle span itself: no cliff after a swell.
fn judge_settle_span(label: &str, run: &[Frame], reference: &[Frame], step_share: f32) {
    let means: Vec<f32> = run[..SETTLE_FRAMES].iter().map(frame_mean).collect();
    let reference_level = reference[..SETTLE_FRAMES]
        .iter()
        .map(frame_mean)
        .sum::<f32>()
        / SETTLE_FRAMES as f32;
    let largest_step = means
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .fold(0.0, f32::max);
    assert!(
        largest_step <= reference_level * step_share,
        "{label}: the frame mean steps by {largest_step:.3} between two frames, against a \
         reference level of {reference_level:.3}: {means:.2?}"
    );
}

/// How far the level a boundary inside a song leaves behind may stray from the
/// run that never had one, over the three to ten seconds after it, as a
/// multiple. The creep that takes over must find the same equilibrium and not
/// settle dim and swell back.
pub const SETTLED_BAND: (f32, f32) = (0.93, 1.07);
/// First and last frame of that stretch.
pub const SETTLED_FRAMES: std::ops::Range<usize> = 180..600;

pub fn assert_settles_where_a_continuing_run_does(
    label: &str,
    run: &[Frame],
    continuing: &[Frame],
) {
    let level = |frames: &[Frame]| {
        frames[SETTLED_FRAMES].iter().map(frame_mean).sum::<f32>() / SETTLED_FRAMES.len() as f32
    };
    let ratio = level(run) / level(continuing);
    assert!(
        (SETTLED_BAND.0..=SETTLED_BAND.1).contains(&ratio),
        "{label}: three to ten seconds on, the level is {ratio:.2} times a run that never had \
         the boundary"
    );
}
