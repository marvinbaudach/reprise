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
//! Three phases follow a boundary:
//! 1. *Waiting.* A bar is computed from a window of audio, so until a full
//!    window of the new stream is in nothing can be measured. The gain holds,
//!    and a frame the held gain would draw as a wall is scaled down to
//!    [`PENDING_CEILING`] (see [`WALL_LEVEL`]).
//! 2. *Tracking.* The gain is set from the loudest raw bar seen since the
//!    boundary, and may only go down from there as a louder bar turns up. A
//!    song's hits are sparse and its intro may be quiet: the first window can
//!    fall between two kicks or inside a fade-in, and a gain taken from it pins
//!    the next loud bar. Going down on the spot costs one frame; waiting for
//!    `cavacore`'s 2 % steps costs seconds. The gain never goes up in this
//!    phase, because "louder than anything seen so far" is not a thing the
//!    audio can say.
//! 3. *Done.* `cavacore`'s own creep, untouched.

/// Height the loudest bar seen is aimed at, as the integral stage settles it.
/// Just under 1.0, the height `cavacore`'s creep holds its loudest bars at.
pub(super) const TARGET_HEIGHT: f32 = 1.0;
/// The height a walled frame is scaled down to while the boundary is still being
/// waited out.
pub(super) const PENDING_CEILING: f32 = 0.85;
/// A waiting frame whose tallest bar would be this many times full height is a
/// wall: the held gain belongs to a song far louder than this one. Anything
/// below it is a shape to continue, which keeps its heights; cavacore's own
/// regulation keeps a settled shape under about 1.3, so a swipe between songs
/// of similar loudness never meets it.
pub(super) const WALL_LEVEL: f32 = 2.0;
/// Tracking lasts this many windows of audio, about seven seconds at 44.1 or
/// 48 kHz. Songs open on a quiet intro or a fade-in, and the gain measured from
/// that must follow the first loud bar down at once; the creep alone would pin
/// the bars for a second or two (2 % per frame, the intro 20 dB down).
pub(super) const TRACKING_WINDOWS: usize = 40;

/// What the smoother does with this frame's gain.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Step {
    /// Hold the gain; the frame is capped.
    Waiting,
    /// Set the gain to `sensitivity`. `first` marks the opening estimate, which
    /// may raise the gain; later ones only lower it.
    Measured { sensitivity: f32, first: bool },
    /// `cavacore`'s creep decides.
    Done,
}

/// Tracks how much of the new stream has been heard and its loudest bar.
pub(super) struct BoundaryEstimator {
    window_samples: usize,
    signal_samples: usize,
    pending: bool,
    estimated: bool,
    peak: f32,
}

impl BoundaryEstimator {
    /// A fresh smoother owes an estimate from its first sample.
    pub(super) fn new(window_samples: usize) -> Self {
        Self {
            window_samples,
            signal_samples: 0,
            pending: true,
            estimated: false,
            peak: 0.0,
        }
    }

    /// The next audio is a different stream; measure it again.
    pub(super) fn arm(&mut self) {
        self.restart_count();
        self.pending = true;
        self.estimated = false;
    }

    /// Replaces the measurement with a gain someone else chose.
    #[cfg(test)]
    pub(super) fn settle(&mut self) {
        self.pending = false;
    }

    /// Feeds one frame: its new samples, whether they carried signal, and its
    /// loudest raw bar. Digital silence restarts the count, because a window
    /// that is mostly silence would understate the level.
    pub(super) fn advance(
        &mut self,
        new_samples: usize,
        signal_present: bool,
        raw_peak: f32,
        integral_feedback: f32,
    ) -> Step {
        if !self.pending {
            return Step::Done;
        }
        if signal_present {
            self.signal_samples = self.signal_samples.saturating_add(new_samples);
            if raw_peak.is_finite() {
                self.peak = self.peak.max(raw_peak);
            }
        } else {
            self.restart_count();
        }
        if self.signal_samples < self.window_samples {
            return Step::Waiting;
        }
        if self.signal_samples >= self.window_samples * TRACKING_WINDOWS {
            self.pending = false;
        }
        match sensitivity_for(self.peak, integral_feedback) {
            Some(sensitivity) => {
                let first = !self.estimated;
                self.estimated = true;
                Step::Measured { sensitivity, first }
            }
            None if self.pending => Step::Waiting,
            None => Step::Done,
        }
    }

    fn restart_count(&mut self) {
        self.signal_samples = 0;
        self.peak = 0.0;
    }
}

/// The gain at which a steady signal whose loudest raw bar is `raw_peak`
/// settles at the target height once the integral stage has added its share
/// (`output = raw * gain / (1 - integral_feedback)`). `None` for a window with
/// no measurable level, which must not be amplified.
fn sensitivity_for(raw_peak: f32, integral_feedback: f32) -> Option<f32> {
    (raw_peak.is_finite() && raw_peak > 0.0)
        .then(|| TARGET_HEIGHT * (1.0 - integral_feedback) / raw_peak)
}
