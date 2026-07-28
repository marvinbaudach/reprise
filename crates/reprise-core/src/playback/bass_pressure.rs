//! Absolute bass-pressure detection for the visualizer's glow layer.
//!
//! The CAVA bars this crate also produces are deliberately relative: CAVA's
//! auto-sensitivity keeps re-normalizing them so the tallest column fills the
//! canvas. That makes them useless as a "how hard is the bass hitting"
//! signal — during a quiet sung passage the gain creeps up until the bass
//! bands read the same as during a drop. This detector therefore measures the
//! PCM directly and never applies a gain of its own: a 30–150 Hz band, its RMS
//! in true dBFS, and a slow baseline of the track's own recent bass.
//!
//! Two readings come out of it. `impact` is what a rhythmic kick reaches, and
//! `aura` only lifts once that pressure is sustained across a breakdown.
//! Calibrated against The Browning — "WAKE UP" and Lorna Shore — "To the
//! Hellfire": the quiet intros measure around −40 dBFS and the walls of sound
//! around −14 dBFS, so absolute level alone separates them by 25 dB.

/// Band the detector listens to — below is rumble/DC, above is no longer bass.
const HIGH_PASS_HZ: f64 = 30.0;
const LOW_PASS_HZ: f64 = 150.0;
/// One analysis window; short enough to catch a kick, long enough to average.
const ANALYSIS_WINDOW_S: f32 = 0.0116;
/// The track's own recent bass, against which a swell is measured.
const BASELINE_TAU_S: f32 = 2.0;
/// How long pressure has to hold before it reads as a breakdown.
const SUSTAIN_TAU_S: f32 = 0.45;
/// Below this the bass band is too quiet to glow at all.
const QUIET_DBFS: f32 = -42.0;
/// At and above this the bass carries a track completely.
const LOUD_DBFS: f32 = -16.0;
/// Swell over the baseline that starts reading as a push, and where it maxes.
const PUSH_MIN_DB: f32 = 2.0;
const PUSH_FULL_DB: f32 = 14.0;
/// Where a loud but unremarkable bass line rests.
pub const STEADY_GLOW: f32 = 0.35;
/// Per-window release, so a kick carries for ~200 ms instead of flickering.
const RELEASE_PER_WINDOW: f32 = 0.06;
/// Sustained pressure below this stays a plain glow, without the inner aura.
const AURA_ONSET: f32 = 0.55;
/// Reported level of digital silence.
const SILENCE_DBFS: f32 = -140.0;
/// Below this the filter state is snapped to zero. Left alone it decays
/// asymptotically into subnormal floats, where arithmetic costs an order of
/// magnitude more — on the audio thread, during every fade-out and every gap
/// between tracks. The threshold sits far above the subnormal range and about
/// 600 dB below anything audible, so nothing real is truncated.
const DENORMAL_FLOOR: f64 = 1.0e-30;

/// One reading of how hard the bass is currently pushing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BassPressure {
    /// Absolute level of the 30–150 Hz band, in dBFS.
    pub level_dbfs: f32,
    /// The track's own slow bass average, in dBFS.
    pub baseline_dbfs: f32,
    /// Glow a kick reaches: `0..=1`, resting at [`STEADY_GLOW`] under load.
    pub impact: f32,
    /// Inner aura of a sustained breakdown: `0..=1`, usually `0`.
    pub aura: f32,
}

impl BassPressure {
    pub(crate) fn silent() -> Self {
        Self {
            level_dbfs: SILENCE_DBFS,
            baseline_dbfs: SILENCE_DBFS,
            impact: 0.0,
            aura: 0.0,
        }
    }

    /// Bounds a reading at the frame boundary: levels stay finite, glow stays
    /// within `0..=1`.
    pub(crate) fn sanitized(self) -> Self {
        let decibels = |value: f32| {
            if value.is_finite() {
                value
            } else {
                SILENCE_DBFS
            }
        };
        let glow = |value: f32| {
            if value.is_finite() {
                value.clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        Self {
            level_dbfs: decibels(self.level_dbfs),
            baseline_dbfs: decibels(self.baseline_dbfs),
            impact: glow(self.impact),
            aura: glow(self.aura),
        }
    }
}

/// Measures bass pressure from the same mono PCM the CAVA bars are built from,
/// without CAVA's auto-sensitivity in the path.
pub struct BassPressureDetector {
    high_pass: Biquad,
    low_pass: Biquad,
    window_len: usize,
    baseline_coeff: f32,
    sustain_coeff: f32,
    square_sum: f64,
    window_samples: usize,
    level_dbfs: f32,
    baseline_dbfs: Option<f32>,
    impact: f32,
    sustain: f32,
}

impl BassPressureDetector {
    pub fn new(sample_rate_hz: u32) -> Self {
        let rate = f64::from(sample_rate_hz.max(1));
        let window_len = ((sample_rate_hz as f32 * ANALYSIS_WINDOW_S).round() as usize).max(1);
        let window_s = window_len as f32 / sample_rate_hz.max(1) as f32;
        Self {
            high_pass: Biquad::high_pass(rate, HIGH_PASS_HZ),
            low_pass: Biquad::low_pass(rate, LOW_PASS_HZ),
            window_len,
            baseline_coeff: ema_coeff(window_s, BASELINE_TAU_S),
            sustain_coeff: ema_coeff(window_s, SUSTAIN_TAU_S),
            square_sum: 0.0,
            window_samples: 0,
            level_dbfs: SILENCE_DBFS,
            baseline_dbfs: None,
            impact: 0.0,
            sustain: 0.0,
        }
    }

    /// Folds one PCM chunk in and reports the current pressure. Chunk sizes are
    /// whatever the audio sink delivers; the analysis window is independent.
    pub fn observe(&mut self, mono_samples: &[f32]) -> BassPressure {
        for sample in mono_samples {
            let sample = if sample.is_finite() {
                f64::from(sample.clamp(-1.0, 1.0))
            } else {
                0.0
            };
            let filtered = self.low_pass.process(self.high_pass.process(sample));
            self.square_sum += filtered * filtered;
            self.window_samples += 1;
            if self.window_samples >= self.window_len {
                self.close_window();
            }
        }
        self.reading()
    }

    /// Drops the previous track's baseline and glow history.
    pub fn reset(&mut self) {
        self.high_pass.reset();
        self.low_pass.reset();
        self.square_sum = 0.0;
        self.window_samples = 0;
        self.level_dbfs = SILENCE_DBFS;
        self.baseline_dbfs = None;
        self.impact = 0.0;
        self.sustain = 0.0;
    }

    fn close_window(&mut self) {
        let mean_square = self.square_sum / self.window_len as f64;
        self.square_sum = 0.0;
        self.window_samples = 0;

        let rms = mean_square.max(0.0).sqrt() as f32;
        self.level_dbfs = if rms > 0.0 {
            (20.0 * rms.log10()).max(SILENCE_DBFS)
        } else {
            SILENCE_DBFS
        };
        // The first window has no history to compare against, so it becomes the
        // baseline instead of reading as an infinite swell.
        let baseline = match self.baseline_dbfs {
            Some(baseline) => baseline + (self.level_dbfs - baseline) * self.baseline_coeff,
            None => self.level_dbfs,
        };
        self.baseline_dbfs = Some(baseline);

        let loudness = smoothstep((self.level_dbfs - QUIET_DBFS) / (LOUD_DBFS - QUIET_DBFS));
        let push =
            smoothstep(((self.level_dbfs - baseline) - PUSH_MIN_DB) / (PUSH_FULL_DB - PUSH_MIN_DB));
        let target = loudness * (STEADY_GLOW + (1.0 - STEADY_GLOW) * push);
        self.impact = target.max(self.impact - RELEASE_PER_WINDOW).clamp(0.0, 1.0);
        self.sustain += (self.impact - self.sustain) * self.sustain_coeff;
    }

    fn reading(&self) -> BassPressure {
        let Some(baseline_dbfs) = self.baseline_dbfs else {
            return BassPressure::silent();
        };
        BassPressure {
            level_dbfs: self.level_dbfs,
            baseline_dbfs,
            impact: self.impact,
            aura: smoothstep((self.sustain - AURA_ONSET) / (1.0 - AURA_ONSET)),
        }
    }
}

fn smoothstep(value: f32) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

/// Single-pole EMA coefficient for one window of `window_s` and time constant
/// `tau_s`.
fn ema_coeff(window_s: f32, tau_s: f32) -> f32 {
    1.0 - (-window_s / tau_s).exp()
}

/// Second-order Butterworth section, direct form I. The state is `f64` because
/// a 30 Hz corner at 44.1 kHz sits close enough to the unit circle that `f32`
/// accumulates audible drift.
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Biquad {
    fn low_pass(sample_rate_hz: f64, cutoff_hz: f64) -> Self {
        let (cos, alpha) = Self::shape(sample_rate_hz, cutoff_hz);
        let a0 = 1.0 + alpha;
        Self::new(
            (1.0 - cos) / 2.0 / a0,
            (1.0 - cos) / a0,
            (1.0 - cos) / 2.0 / a0,
            -2.0 * cos / a0,
            (1.0 - alpha) / a0,
        )
    }

    fn high_pass(sample_rate_hz: f64, cutoff_hz: f64) -> Self {
        let (cos, alpha) = Self::shape(sample_rate_hz, cutoff_hz);
        let a0 = 1.0 + alpha;
        Self::new(
            (1.0 + cos) / 2.0 / a0,
            -(1.0 + cos) / a0,
            (1.0 + cos) / 2.0 / a0,
            -2.0 * cos / a0,
            (1.0 - alpha) / a0,
        )
    }

    /// Returns `(cos ω₀, α)`. `Q = 1/√2` is what makes the section Butterworth
    /// (maximally flat in the passband).
    fn shape(sample_rate_hz: f64, cutoff_hz: f64) -> (f64, f64) {
        let w0 = std::f64::consts::TAU * cutoff_hz / sample_rate_hz;
        let (sin, cos) = w0.sin_cos();
        (cos, sin / std::f64::consts::SQRT_2)
    }

    fn new(b0: f64, b1: f64, b2: f64, a1: f64, a2: f64) -> Self {
        Self {
            b0,
            b1,
            b2,
            a1,
            a2,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn process(&mut self, x: f64) -> f64 {
        let mut y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        if y.abs() < DENORMAL_FLOOR {
            y = 0.0;
        }
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }

    fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_settling_filter_snaps_to_zero_instead_of_drifting_into_denormals() {
        let mut biquad = Biquad::low_pass(44_100.0, LOW_PASS_HZ);
        for _ in 0..1_000 {
            biquad.process(1.0);
        }

        // One second of digital silence — a track gap or the end of a fade.
        for _ in 0..44_100 {
            biquad.process(0.0);
        }

        assert_eq!(biquad.y1, 0.0);
        assert_eq!(biquad.y2, 0.0);
    }
}
