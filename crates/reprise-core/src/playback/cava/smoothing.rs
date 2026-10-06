const INITIAL_FRAMERATE: f32 = 75.0;
const CAVA_REFERENCE_FRAMERATE: f32 = 66.0;
const FALL_STEP: f32 = 0.028;
const MAX_SENSITIVITY: f32 = 1_000_000.0;
const MIN_SENSITIVITY: f32 = 1.0e-6;
const MAX_INTERNAL_BAR_VALUE: f32 = 64.0;
const MAX_INTEGRAL_FEEDBACK: f32 = 0.98;
/// The most a measured gain may rise in one frame on its way up to its goal.
const GOAL_RISE_PER_FRAME: f32 = 1.6;

use super::boundary::{BoundaryEstimator, Step};

pub(super) struct Smoother {
    noise_reduction: f32,
    autosensitivity: u32,
    sensitivity: f32,
    boundary: BoundaryEstimator,
    framerate: f32,
    frame_skip: u32,
    previous: Vec<f32>,
    peaks: Vec<f32>,
    fall: Vec<f32>,
    memory: Vec<f32>,
    /// A gain the measurement found and the smoother is still moving up to.
    goal: Option<f32>,
    /// Bars the new stream has written since the boundary.
    driven: Vec<bool>,
}

impl Smoother {
    /// `window_samples` is the audio a boundary estimate waits for: the FFT
    /// input buffer, which is what every bar is computed from.
    pub(super) fn new(
        bar_count: usize,
        noise_reduction: f32,
        autosensitivity: u32,
        window_samples: usize,
    ) -> Self {
        Self {
            noise_reduction,
            autosensitivity,
            sensitivity: 1.0,
            boundary: BoundaryEstimator::new(window_samples),
            framerate: INITIAL_FRAMERATE,
            frame_skip: 1,
            previous: vec![0.0; bar_count],
            peaks: vec![0.0; bar_count],
            fall: vec![0.0; bar_count],
            memory: vec![0.0; bar_count],
            goal: None,
            driven: vec![false; bar_count],
        }
    }

    pub(super) fn apply(
        &mut self,
        bars: &mut [f32],
        new_samples: usize,
        sample_rate_hz: u32,
        signal_present: bool,
    ) {
        self.update_framerate(new_samples, sample_rate_hz);
        let framerate_mod = CAVA_REFERENCE_FRAMERATE / self.framerate;
        let integral_feedback = self.integral_feedback(framerate_mod);
        let gravity_mod = (self.noise_reduction > 0.1)
            .then(|| framerate_mod.powf(2.5) * 2.0 / self.noise_reduction);
        let mut measuring = false;
        if self.autosensitivity > 0 {
            let raw_peak = bars
                .iter()
                .copied()
                .filter(|bar| bar.is_finite())
                .fold(0.0, f32::max);
            let step = self.boundary.advance(
                new_samples,
                signal_present,
                raw_peak,
                integral_feedback,
                self.sensitivity,
            );
            match step {
                Step::Hold => measuring = true,
                Step::Measure(sensitivity) => {
                    measuring = true;
                    self.sensitivity = sensitivity.clamp(MIN_SENSITIVITY, MAX_SENSITIVITY);
                }
                Step::Goal(sensitivity) => {
                    self.goal = Some(sensitivity.clamp(MIN_SENSITIVITY, MAX_SENSITIVITY));
                }
                Step::Brake { trigger, target } => {
                    if self.sensitivity > trigger {
                        self.brake_to(target);
                    }
                }
                Step::Done => {}
            }
        }

        if let Some(goal) = self.goal {
            measuring = true;
            if goal <= self.sensitivity {
                self.brake_to(goal);
                self.goal = None;
            } else {
                let risen = (self.sensitivity * GOAL_RISE_PER_FRAME).min(goal);
                self.rescale_driven(risen / self.sensitivity);
                self.sensitivity = risen;
                if self.sensitivity >= goal {
                    self.goal = None;
                }
            }
        }

        let mut overshoot = false;
        for (bar, (((previous, peak), fall), memory)) in bars.iter_mut().zip(
            self.previous
                .iter_mut()
                .zip(self.peaks.iter_mut())
                .zip(self.fall.iter_mut())
                .zip(self.memory.iter_mut()),
        ) {
            if self.autosensitivity > 0 {
                *bar *= self.sensitivity;
            }
            if !bar.is_finite() {
                *bar = 0.0;
            }
            if *bar < *previous {
                if let Some(gravity_mod) = gravity_mod {
                    *bar = (*peak * (1.0 - *fall * *fall * gravity_mod)).max(0.0);
                    *fall += FALL_STEP;
                } else {
                    *peak = *bar;
                    *fall = 0.0;
                }
            } else {
                *peak = *bar;
                *fall = 0.0;
            }
            *previous = *bar;
            *bar += *memory * integral_feedback;
            if !bar.is_finite() {
                *bar = 0.0;
                *previous = 0.0;
                *peak = 0.0;
                *fall = 0.0;
                *memory = 0.0;
            } else {
                *memory = bar.clamp(0.0, MAX_INTERNAL_BAR_VALUE);
                overshoot |= *bar > 1.0;
            }
        }

        if self.boundary.is_collecting() || self.goal.is_some() {
            // A bar that rose this frame (`fall` restarted) was written by the
            // new stream; one falling by gravity still carries the old one.
            for (driven, fall) in self.driven.iter_mut().zip(&self.fall) {
                *driven |= *fall == 0.0;
            }
        } else {
            self.driven.fill(true);
        }

        // While the gain is being set from the new stream, creeping on a stale
        // overshoot or climbing from a cold start is the pumping that
        // replaces.
        if self.autosensitivity > 0 && !measuring {
            if overshoot {
                let reduction = (1.0 - 0.02 * framerate_mod).max(0.01);
                self.sensitivity *= reduction;
            } else if signal_present {
                self.sensitivity *= 1.0 + 0.001 * framerate_mod * self.autosensitivity as f32;
            }
            self.sensitivity = self.sensitivity.clamp(MIN_SENSITIVITY, MAX_SENSITIVITY);
        }

        for bar in bars {
            *bar = bar.clamp(0.0, 1.0);
        }
    }

    /// Lowers the gain at once. A frame that needs braking is louder than what
    /// a pending goal was measured from, so the goal is stale.
    fn brake_to(&mut self, sensitivity: f32) {
        let sensitivity = sensitivity.clamp(MIN_SENSITIVITY, MAX_SENSITIVITY);
        self.goal = None;
        self.rescale_driven(sensitivity / self.sensitivity);
        self.sensitivity = sensitivity;
    }

    /// The bar history is in units of the gain that produced it. A bar the new
    /// stream has written since the boundary holds its integral and gravity
    /// state at the old gain, so it is converted to the new one; otherwise the
    /// next frame would add the old gain's memory, or hold a gravity peak, on
    /// top of its own. A bar still falling from the previous stream is already
    /// in screen units and keeps its value.
    fn rescale_driven(&mut self, ratio: f32) {
        for (index, driven) in self.driven.iter().enumerate() {
            if *driven {
                self.previous[index] *= ratio;
                self.peaks[index] *= ratio;
                self.memory[index] *= ratio;
            }
        }
    }

    /// Clears the bar history (`previous`/`peaks`/`fall`/`memory`) and owes a
    /// new boundary estimate. `framerate` and `frame_skip` are kept: the
    /// device's output frame rate does not change across a boundary.
    pub(super) fn reset(&mut self) {
        self.previous.fill(0.0);
        self.peaks.fill(0.0);
        self.fall.fill(0.0);
        self.memory.fill(0.0);
        self.goal = None;
        self.driven.fill(false);
        self.boundary.arm(false);
    }

    #[cfg(test)]
    pub(super) fn sensitivity(&self) -> f32 {
        self.sensitivity
    }

    /// Replaces the gain and ends any pending estimate, so a test can start the
    /// smoother from a gain `cavacore` itself reached.
    #[cfg(test)]
    pub(super) fn adopt_sensitivity(&mut self, sensitivity: f32) {
        self.sensitivity = sensitivity;
        self.goal = None;
        self.boundary.settle();
    }

    /// Owes a new boundary estimate and keeps the bar history, so the next
    /// frames fall from the shape already on screen.
    pub(super) fn rearm_boundary(&mut self) {
        self.goal = None;
        self.driven.fill(false);
        self.boundary.arm(true);
    }

    /// Seeds the smoother with a shape a viewer has already seen, so the next
    /// frame continues it. Leaves the autosensitivity gain, the pending
    /// boundary estimate and `framerate` untouched. Shorter input than
    /// `bar_count` seeds only its own bars; longer input is truncated by `zip`.
    ///
    /// `bars` is the displayed shape, approximately the smoother's own
    /// output: on Android it is the visual engine's `current_bands()`, which
    /// may already be decayed or blended toward idle. `apply` builds its
    /// output as `bar + memory * integral_feedback`, so the displayed shape
    /// has the integral term in it, while `previous`/`peaks` hold the bar
    /// *before* that term is added. The state whose next frame reproduces
    /// `bars` is therefore `memory = bars` and
    /// `previous = peaks = bars * (1 - integral_feedback)`, with `fall`
    /// restarted so the first falling frame equals its peak. Storing `bars`
    /// itself in `previous`/`peaks` would draw the next frame at about
    /// `1 / (1 - integral_feedback)` times the shape, clip it, and drive the
    /// autosensitivity gain down.
    ///
    /// Two limits on "continues":
    /// - A seed on a fresh smoother is continued by the new stream's own
    ///   level, not by a gain: the first frames are normalized to the audio
    ///   that has arrived, so the seed's memory feeds a ramp, not a held shape.
    /// - `integral_feedback` is evaluated with the framerate from before the
    ///   next `apply` runs `update_framerate`. Normally the difference is about
    ///   1e-3; a tiny first chunk can push the real feedback toward the 0.98
    ///   cap and overshoot that one frame.
    pub(super) fn seed_shape(&mut self, bars: &[f32]) {
        let feedback = self.integral_feedback(CAVA_REFERENCE_FRAMERATE / self.framerate);
        let seeded = self
            .previous
            .iter_mut()
            .zip(self.peaks.iter_mut())
            .zip(self.fall.iter_mut())
            .zip(self.memory.iter_mut())
            .zip(bars.iter());
        for ((((previous, peak), fall), memory), bar) in seeded {
            let shown = (if bar.is_finite() { *bar } else { 0.0 }).clamp(0.0, 1.0);
            let pre_integral = shown * (1.0 - feedback);
            *previous = pre_integral;
            *peak = pre_integral;
            *fall = 0.0;
            *memory = shown;
        }
    }

    /// The share of last frame's output that `apply` adds to this frame.
    fn integral_feedback(&self, framerate_mod: f32) -> f32 {
        let integral_mod = framerate_mod.powf(0.1);
        (self.noise_reduction / integral_mod).min(MAX_INTEGRAL_FEEDBACK)
    }

    fn update_framerate(&mut self, new_samples: usize, sample_rate_hz: u32) {
        if new_samples == 0 {
            self.frame_skip = self.frame_skip.saturating_add(1);
            return;
        }
        self.framerate -= self.framerate / 64.0;
        self.framerate +=
            sample_rate_hz as f32 * self.frame_skip as f32 / new_samples as f32 / 64.0;
        self.frame_skip = 1;
    }
}

#[cfg(test)]
mod boundary_tests;
#[cfg(test)]
mod tests;
