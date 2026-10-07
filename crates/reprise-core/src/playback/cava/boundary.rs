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
//! louder bar after a quiet start is followed down on the spot, from ten or
//! forty times the measured gain in the first frame to the measured one. The
//! bar history those frames drew is in the units of the gain that drew it, so
//! each new gain rescales it, as the brakes and the move up do; left alone, the history of
//! the high early gains is added to every later frame and the first 0.3 s are
//! drawn at up to twice the level a settled engine draws. The measurement ends
//! when a full window of signal is in. Silence restarts it,
//! because a window that is mostly silence understates the level, until the
//! measurement has gathered [`MEASURING_CAP_WINDOWS`] windows of audio since its
//! first signal, silent or not: from there on a silence only pauses it, unless
//! it is a whole window long, which is a break and restarts it. Music that
//! falls to digital silence every tenth of a second never gathers a window of
//! signal between two gaps, and a measurement that restarts on each of them
//! would set the gain from every frame's own tallest bar for as long as the
//! pattern lasts (or, after a track change, hold the last song's gain for as
//! long as the gating does). What can still keep a measurement from finishing
//! is a stretch of signal shorter than a window between silences of a window
//! or more: audio that is mostly silence, which the restart is for.
//!
//! *A shape on screen* (a track change or a seek). The gain that drew it is
//! kept as a prior, because the song just before is usually about as loud as
//! this one and swapping the gain draws the whole frame at another height. The
//! loudest bar of the new stream so far is the evidence. While the window
//! fills the gain is held, not creeping. If the gain would draw the new stream
//! as a wall (the measurement says it is more than [`CARRY_BAND`] too high) the
//! measurement takes over at once, as above, without waiting for the window,
//! and without rescaling the history: the shape on screen is not to jump when
//! the gain under it is replaced. If it would draw it dim (more than
//! [`CARRY_BAND`] too low) the gain moves up to the measurement over a few
//! frames once the window is in. Within the band the gain stays.
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
//! after a brake, a frame is braked as soon as the gain would draw it at full
//! height, and it lands at [`CHAIN_TARGET_HEIGHT`], above the shared target:
//! the chain is following a rise, and a landing at the shared target leaves
//! the rest of it dim. Each brake of the chain extends the span; a stream the
//! gain draws under full height lets it lapse, and only a brake starts it.
//!
//! What the measurement cannot settle is the first window's own reading. It is
//! the loudest bar of about 0.2 s of audio, and the gain `cavacore` settles at
//! lands that bar anywhere from half to full height (median 0.77 of full height,
//! the middle 80 % of windows between 0.52 and 1.06 on real music; the same at
//! 44.1 and 48 kHz): the measurement lands above that equilibrium in about
//! 65 % of the windows and more than 1.3 times above it in 30 %. A fresh start
//! therefore reads a median 1.17 times a settled engine's level over 0.3 to 1 s
//! after the boundary, 1.56 times at the 90th percentile, until the creep
//! brings it down; a replaced gain lands above equilibrium in most windows and
//! below it in the rest, and pins more frames than the settled engine. No
//! landing height fixes this, because the error changes sign from one window to
//! the next, and a longer measurement trades it for more pinned frames in a
//! fade-in; it is accepted, and AC-29 states it.
//!
//! The same reading keeps the dead zone of the carry band as it is. A drop of 3
//! to 7.5 dB keeps the carried gain when the measurement reads under twice too
//! low, and the new song stays dim until the creep climbs back: a median 0.74
//! times a settled engine's level at 3 and 4.5 dB, about half the cases under
//! 0.75. A dim side narrowed to 1.7 times lifts those, and pins more frames
//! doing so, because the gain it moves to is the measurement's. The project
//! prefers dim to pinned (the brakes above land under full height for the same
//! reason), so the band stays at twice until a measurement closer to
//! equilibrium exists.
//!
//! What is left is dimming, not pinning. A rise inside the span (a verse
//! giving way to a chorus eight or ten decibels up, say) reads under the level
//! a gain settled on the whole song would draw it at, for the seconds the creep
//! needs to climb back at 6 % a second: about 0.6 times in the first second and
//! 0.74 to 0.9 times in the next few, on real music. AC-29 states the figures.
//!
//! Nothing else changes: once the measurement, or the move up to it, is over,
//! `cavacore`'s creep runs in both directions, so the gain settles where it
//! would have without a boundary, and silence neither restarts nor prolongs
//! the span. The cap that ends the restarts counts from the first signal; the
//! span counts from the end of the measurement, so the time spent collecting is
//! not charged to it.

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
/// factor of it, either way. The dim side could be narrower, a dim line costing
/// seconds where a wall is braked at once, but every gain the measurement
/// replaces lands above `cavacore`'s equilibrium more often than below it (see
/// the module docs) and pins more frames than the settled engine does, so a
/// narrower dim side trades dim for pinned: a 6 dB drop replaced on a real pair
/// of songs pins 1061 frames over 0 to 3 s against the settled engine's 720.
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
/// windows of audio, about 1.1 s, after the last brake, a frame is braked at
/// [`CHAIN_BRAKE_LEVEL`]. It spans more than a beat of slow music: the loudest
/// bar of a fade-in is set by the hits, and a hit is a half second or a second
/// apart.
pub(super) const CHAIN_WINDOWS: usize = 6;
/// A frame in a chain is braked once the gain would draw it at full height.
pub(super) const CHAIN_BRAKE_LEVEL: f32 = 1.0;
/// Height a chain brake lands the frame at. Higher than [`TARGET_HEIGHT`]
/// because a chain follows a rise that is still going: every brake that lands
/// at the shared target leaves the gain a little low for the rise's next step,
/// and the creep needs seconds to climb back. Under [`CHAIN_BRAKE_LEVEL`], or a
/// brake would raise the gain it is there to lower. Real music after a rise of
/// 8 to 10 dB reads about 0.05 closer to the level of a gain settled on the
/// song at 0.92 than at 0.85. 0.95 reads about the same (better after a 10 dB
/// step, worse after an 8 dB one) and also passes the fade-in bound, but with
/// no pinned frame to spare after a track change (as many as the settled gain
/// draws, against six fewer at 0.92), so 0.92 is the tie-break.
pub(super) const CHAIN_TARGET_HEIGHT: f32 = 0.92;

/// Audio, in windows, a measurement gathers from its first signal, silent or
/// not, before silence stops restarting it and only pauses it: about 0.7 s at
/// 44.1 or 48 kHz. A gap before it is a break in the music and the window starts
/// again; one after it is part of the music's rhythm, and a measurement that
/// restarted on it would never finish where the gaps recur faster than a window
/// fills (hard-gated electronic music, chiptune). With one silent hop of 735
/// samples in ten the measurement ends 0.9 s in. A silence of a whole window is
/// a break whenever it comes: it restarts the measurement, without taking back
/// what the cap has counted.
///
/// Four, because the cap has to outlast the first half second, in which a
/// dropout is most likely a break (the early braking stretch is
/// [`EARLY_BRAKING_WINDOWS`] windows) and the window of music behind it is still
/// to fill, and because every window more is another 0.17 s in which gated music
/// keeps the per-frame gain: the measurement of such music ends 0.7 s in at
/// three windows, 0.9 s at four and 1.1 s at five. Real music from a first
/// start or after a track change reads the same, seconds 15 to 30, at two to
/// eight windows.
///
/// The count starts at the first signal of the boundary, so a lead-in of
/// digital silence (47 % of the tracks of a real collection have one, up to
/// 1.8 s) is not charged to it: nothing has been gathered that a restart would
/// lose, and a cap used up by a lead-in would turn the first dropout of the
/// music behind it into a pause.
///
/// It has a counter of its own: `elapsed_samples` counts from the end of the
/// measurement, and both braking spans are set by it.
pub(super) const MEASURING_CAP_WINDOWS: usize = 4;

/// What the smoother does with this frame's gain.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Step {
    /// No signal to measure: hold the gain.
    Hold,
    /// Set the gain to `sensitivity`. With nothing on screen
    /// (`rescales_history`) the bar history follows it; under a shape that is
    /// kept the history stays, or the frame would jump.
    Measure {
        sensitivity: f32,
        rescales_history: bool,
    },
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
    /// Audio since the first signal of the boundary, silent or not, while the
    /// measurement collects. Unlike `elapsed_samples`, which counts the span of
    /// braking that follows and is not charged for the time spent collecting.
    gathered_samples: usize,
    /// Silent samples in a row, since the last chunk with signal.
    silent_run: usize,
    elapsed_samples: usize,
    /// Audio left in the span after a brake in which a frame is braked at
    /// [`CHAIN_BRAKE_LEVEL`].
    chain_samples: usize,
    peak: f32,
    phase: Phase,
    /// A shape was on screen when the boundary was armed, and its gain is a
    /// prior. Decides whether a measured gain rescales the bar history.
    keeps_shape: bool,
}

impl BoundaryEstimator {
    /// A fresh smoother owes a measurement from its first sample and has no
    /// gain worth keeping.
    pub(super) fn new(window_samples: usize) -> Self {
        let mut estimator = Self {
            window_samples,
            signal_samples: 0,
            gathered_samples: 0,
            silent_run: 0,
            elapsed_samples: 0,
            chain_samples: 0,
            peak: 0.0,
            phase: Phase::Measuring,
            keeps_shape: false,
        };
        estimator.arm(false);
        estimator
    }

    /// The next audio is a different stream; measure it again. `keeps_shape`
    /// says whether a shape is on screen to continue, and so whether the gain
    /// that drew it is a prior to keep unless the new stream disagrees.
    pub(super) fn arm(&mut self, keeps_shape: bool) {
        self.signal_samples = 0;
        self.gathered_samples = 0;
        self.silent_run = 0;
        self.elapsed_samples = 0;
        self.chain_samples = 0;
        self.peak = 0.0;
        self.keeps_shape = keeps_shape;
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

    pub(super) fn is_waiting(&self) -> bool {
        self.phase == Phase::Waiting
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
        match self.phase {
            // Settled: an empty chunk is no reason to suspend the creep.
            Phase::Done => Step::Done,
            _ if new_samples == 0 => Step::Hold,
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
        // The cap counts from the first signal: a lead-in of silence has nothing
        // gathered that a restart would lose, so it is not charged to it.
        if signal_present || self.gathered_samples > 0 {
            self.gathered_samples = self.gathered_samples.saturating_add(new_samples);
        }
        if !signal_present {
            // Until the cap, a gap restarts the measurement; from it on, a gap
            // only pauses it, so signal that keeps being cut short can still
            // gather a window. A silence as long as a window is no gap in the
            // music but a break: nothing of what was gathered is in the FFT
            // window any more, so it restarts the measurement at any time. It
            // leaves `gathered_samples` alone, or a pattern of breaks could
            // keep the cap from ever being reached.
            self.silent_run = self.silent_run.saturating_add(new_samples);
            if self.gathered_samples < self.window_samples * MEASURING_CAP_WINDOWS
                || self.silent_run >= self.window_samples
            {
                self.signal_samples = 0;
                self.peak = 0.0;
            }
            return Step::Hold;
        }
        self.silent_run = 0;
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
            (false, Some(sensitivity)) => Step::Measure {
                sensitivity,
                rescales_history: !self.keeps_shape,
            },
            // The carried gain draws this stream as a wall: stop trusting it.
            (true, Some(sensitivity)) if sensitivity < gain / CARRY_BAND => {
                if !full {
                    self.phase = Phase::Measuring;
                }
                Step::Measure {
                    sensitivity,
                    rescales_history: false,
                }
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
        let chaining = self.chain_samples > 0;
        let level = if chaining {
            CHAIN_BRAKE_LEVEL
        } else if self.elapsed_samples < self.window_samples * EARLY_BRAKING_WINDOWS {
            EARLY_BRAKE_LEVEL
        } else {
            BRAKE_LEVEL
        };
        let trigger = lands_at_full * level;
        if gain > trigger {
            self.chain_samples = self.window_samples * CHAIN_WINDOWS;
        }
        let height = if chaining {
            CHAIN_TARGET_HEIGHT
        } else {
            TARGET_HEIGHT
        };
        Step::Brake {
            trigger,
            target: lands_at_full * height,
        }
    }
}
