//! The sensitivity measured at a stream boundary.
//!
//! `cavacore` finds its gain by creeping: up 0.1 % per frame, down 2 % on an
//! overshoot, and a cold start climbs 10 % per frame until the first overshoot.
//! That is right for one endless stream and wrong at a boundary. A new song's
//! loudness is unrelated to the last one's, so a carried gain draws a louder
//! song as a wall of pinned bars and a quieter one as a flat line for seconds,
//! and the cold climb swells the whole frame up and back down. Here the
//! smoother instead reads the new audio's own level, sets the gain so its
//! loudest bar lands at a target height, and hands back to `cavacore`'s creep.
//!
//! What happens depends on whether a shape is on screen to continue.
//!
//! *Nothing on screen* (a new processor, or a hard restart). The gain is set
//! every frame from the loudest raw bar seen since the boundary. Each frame's
//! bars are normalized by that peak, so the window-fill ramp the FFT produces
//! rises through the integral stage to the song's level without drawing it at
//! a gain it has not earned. The peak only grows, so the gain only falls: a
//! louder bar after a quiet start is followed down on the spot. The
//! measurement ends when a full window of signal is in. Silence restarts it,
//! because a window that is mostly silence understates the level.
//!
//! *A shape on screen* (a track change or a seek). The gain that drew it is
//! kept as a prior, because the song just before is usually about as loud as
//! this one and swapping the gain draws the whole frame at another height. The
//! loudest bar of the new stream so far is the evidence. While the window
//! fills the gain is held, not creeping. If the gain would draw the new stream
//! as a wall (the measurement says it is more than [`CARRY_BAND`] too high) the
//! measurement takes over at once, as above, without waiting for the window. If
//! it would draw it dim (more than [`CARRY_BAND`] too low) the gain moves up to
//! the measurement over a few frames once the window is in. Within the band the
//! gain stays.
//!
//! *Braking* then lasts about fourteen seconds on either path. A frame the gain
//! would draw at [`BRAKE_LEVEL`] times full height or more (a song that opened
//! quietly and now drops in) pulls the gain down to land at the target height
//! at once; the creep alone would pin the bars for seconds. The span has to
//! outlast the longest quiet opening it is meant to catch, because an intro
//! looks like a quiet song until its body arrives: eight or ten seconds of it
//! is still the gain's to answer for. For the first half second the threshold
//! is [`EARLY_BRAKE_LEVEL`], because a first window that fell between two hits
//! is most often wrong by a little, early.
//!
//! A brake also says the rise may not be over. The body fills the FFT window
//! over the frames that follow, and a fade-in rises for seconds, each frame a
//! little over the last, so no frame is ever a gross overshoot and the creep
//! pins the bars while it catches up. For [`CHAIN_WINDOWS`] windows of audio
//! after a brake, a frame louder than anything the stream has shown since the
//! boundary (a new high) is braked as soon as the gain would draw it at full
//! height, and each new high or brake extends the span. A stream that stops
//! setting highs ends it, and only a brake starts it.
//!
//! Nothing else changes: once the measurement, or the move up to it, is over,
//! `cavacore`'s creep runs in both directions, so the gain settles where it
//! would have without a boundary, and silence neither restarts nor prolongs
//! the span.

/// Height the loudest bar seen while measuring is aimed at, as the integral
/// stage settles it. Below full height because the first window can fall
/// between two hits, and a gain taken from it that lands the loudest bar it saw
/// at 1.0 pins the next, louder one; above the level that would leave the creep
/// to climb back at 6 % a second.
pub(super) const TARGET_HEIGHT: f32 = 0.85;
/// A frame whose tallest bar the gain would draw at this multiple of full height
/// or more is a drop the gain was not measured for. `cavacore`'s 2 % steps
/// would take seconds to get such a frame under control.
pub(super) const BRAKE_LEVEL: f32 = 2.0;
/// A carried gain is kept while the new stream's measurement stays within this
/// factor of it, either way.
pub(super) const CARRY_BAND: f32 = 2.0;
/// For the first stretch after the window the evidence is thin: a first window
/// that fell between two hits gave a gain the next hit overshoots, and a hit
/// drawn at 1.3 times full height is already a pinned bar and a visible swell.
/// Braking is tighter then.
pub(super) const EARLY_BRAKE_LEVEL: f32 = 1.3;
/// The early stretch lasts this many windows of audio after the first, about
/// half a second at 44.1 or 48 kHz.
pub(super) const EARLY_BRAKING_WINDOWS: usize = 3;
/// Braking lasts this many windows of audio, about fourteen seconds at 44.1 or
/// 48 kHz, counted on every sample, silent or not. Longer than the quiet
/// intros it is meant to catch, which are indistinguishable from a quiet song
/// until the body arrives; the golden test of `cavacore`'s steady state leaves
/// the span through `adopt_sensitivity`, so a span this long is not covered by
/// it.
pub(super) const BRAKING_WINDOWS: usize = 80;
/// A brake means the gain was measured on something quieter than what is
/// arriving, and the rise may not be over (see the module docs). For this many
/// windows of audio, about 1.1 s, after the last brake or new high, a new high
/// is braked at [`CHAIN_BRAKE_LEVEL`]. It spans more than a beat of slow music:
/// the loudest bar of a fade-in is set by the hits, and a hit is a half second
/// or a second apart.
pub(super) const CHAIN_WINDOWS: usize = 6;
/// A new high in a chain is braked once the gain would draw it at full height.
pub(super) const CHAIN_BRAKE_LEVEL: f32 = 1.0;

/// What the smoother does with this frame's gain.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Step {
    /// No signal to measure: hold the gain.
    Hold,
    /// Set the gain to `sensitivity`.
    Measure(f32),
    /// Move the gain to `sensitivity`: down at once, up by a bounded factor a
    /// frame, so a carried gain that was too low does not jump.
    Goal(f32),
    /// If the gain is above `trigger`, set it to `target`.
    Brake { trigger: f32, target: f32 },
    /// `cavacore`'s creep decides.
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    /// Nothing is on screen to continue: the gain is set every frame.
    Measuring,
    /// A shape is on screen and its gain is kept, as a prior, until a window of
    /// the new stream says it is off by more than [`CARRY_BAND`].
    Waiting,
    Braking,
    Done,
}

/// Tracks how much of the new stream has been heard and its loudest bar.
pub(super) struct BoundaryEstimator {
    window_samples: usize,
    signal_samples: usize,
    elapsed_samples: usize,
    /// Audio left in the span after a brake that a new high brakes again.
    chain_samples: usize,
    peak: f32,
    phase: Phase,
}

impl BoundaryEstimator {
    /// A fresh smoother owes a measurement from its first sample and has no
    /// gain worth keeping.
    pub(super) fn new(window_samples: usize) -> Self {
        let mut estimator = Self {
            window_samples,
            signal_samples: 0,
            elapsed_samples: 0,
            chain_samples: 0,
            peak: 0.0,
            phase: Phase::Measuring,
        };
        estimator.arm(false);
        estimator
    }

    /// The next audio is a different stream; measure it again. `keeps_shape`
    /// says whether a shape is on screen to continue, and so whether the gain
    /// that drew it is a prior to keep unless the new stream disagrees.
    pub(super) fn arm(&mut self, keeps_shape: bool) {
        self.signal_samples = 0;
        self.elapsed_samples = 0;
        self.chain_samples = 0;
        self.peak = 0.0;
        self.phase = if keeps_shape {
            Phase::Waiting
        } else {
            Phase::Measuring
        };
    }

    /// Replaces the measurement with a gain someone else chose.
    #[cfg(test)]
    pub(super) fn settle(&mut self) {
        self.phase = Phase::Done;
    }

    /// Whether the window of the new stream is still filling.
    pub(super) fn is_collecting(&self) -> bool {
        matches!(self.phase, Phase::Measuring | Phase::Waiting)
    }

    /// Feeds one frame: its new samples, whether they carried signal, its
    /// loudest raw bar, and the gain that drew it.
    pub(super) fn advance(
        &mut self,
        new_samples: usize,
        signal_present: bool,
        raw_peak: f32,
        integral_feedback: f32,
        gain: f32,
    ) -> Step {
        if new_samples == 0 {
            return Step::Hold;
        }
        match self.phase {
            Phase::Done => Step::Done,
            Phase::Measuring | Phase::Waiting => self.collect(
                new_samples,
                signal_present,
                raw_peak,
                integral_feedback,
                gain,
            ),
            Phase::Braking => self.brake(new_samples, raw_peak, integral_feedback, gain),
        }
    }

    fn collect(
        &mut self,
        new_samples: usize,
        signal_present: bool,
        raw_peak: f32,
        integral_feedback: f32,
        gain: f32,
    ) -> Step {
        let waiting = self.phase == Phase::Waiting;
        if !signal_present {
            self.signal_samples = 0;
            self.peak = 0.0;
            return Step::Hold;
        }
        self.signal_samples = self.signal_samples.saturating_add(new_samples);
        if raw_peak.is_finite() {
            self.peak = self.peak.max(raw_peak);
        }
        let measured =
            (self.peak > 0.0).then(|| TARGET_HEIGHT * (1.0 - integral_feedback) / self.peak);
        let full = self.signal_samples >= self.window_samples;
        if full {
            self.phase = Phase::Braking;
        }
        match (waiting, measured) {
            (_, None) => Step::Hold,
            (false, Some(sensitivity)) => Step::Measure(sensitivity),
            // The carried gain draws this stream as a wall: stop trusting it.
            (true, Some(sensitivity)) if sensitivity < gain / CARRY_BAND => {
                if !full {
                    self.phase = Phase::Measuring;
                }
                Step::Measure(sensitivity)
            }
            // Too low: the window is the evidence, and the gain moves to it.
            (true, Some(sensitivity)) if full && sensitivity > gain * CARRY_BAND => {
                Step::Goal(sensitivity)
            }
            (true, Some(_)) if full => self.brake_step(raw_peak, integral_feedback, gain),
            (true, Some(_)) => Step::Hold,
        }
    }

    fn brake(
        &mut self,
        new_samples: usize,
        raw_peak: f32,
        integral_feedback: f32,
        gain: f32,
    ) -> Step {
        self.elapsed_samples = self.elapsed_samples.saturating_add(new_samples);
        self.chain_samples = self.chain_samples.saturating_sub(new_samples);
        if self.elapsed_samples >= self.window_samples * BRAKING_WINDOWS {
            self.phase = Phase::Done;
        }
        self.brake_step(raw_peak, integral_feedback, gain)
    }

    fn brake_step(&mut self, raw_peak: f32, integral_feedback: f32, gain: f32) -> Step {
        if !(raw_peak.is_finite() && raw_peak > 0.0) {
            return Step::Brake {
                trigger: f32::INFINITY,
                target: f32::INFINITY,
            };
        }
        let lands_at_full = (1.0 - integral_feedback) / raw_peak;
        let new_high = raw_peak > self.peak;
        self.peak = self.peak.max(raw_peak);
        let chaining = new_high && self.chain_samples > 0;
        let level = if chaining {
            CHAIN_BRAKE_LEVEL
        } else if self.elapsed_samples < self.window_samples * EARLY_BRAKING_WINDOWS {
            EARLY_BRAKE_LEVEL
        } else {
            BRAKE_LEVEL
        };
        let trigger = lands_at_full * level;
        if chaining || gain > trigger {
            self.chain_samples = self.window_samples * CHAIN_WINDOWS;
        }
        Step::Brake {
            trigger,
            target: lands_at_full * TARGET_HEIGHT,
        }
    }
}
