const INITIAL_FRAMERATE: f32 = 75.0;
const CAVA_REFERENCE_FRAMERATE: f32 = 66.0;
const FALL_STEP: f32 = 0.028;
const MAX_SENSITIVITY: f32 = 1_000_000.0;
const MIN_SENSITIVITY: f32 = 1.0e-6;
const MAX_INTERNAL_BAR_VALUE: f32 = 64.0;
const MAX_INTEGRAL_FEEDBACK: f32 = 0.98;
const INITIAL_SENSITIVITY_HEADROOM: f32 = 0.85;

pub(super) struct Smoother {
    noise_reduction: f32,
    noise_floor: f32,
    autosensitivity: u32,
    sensitivity: f32,
    sensitivity_initializing: bool,
    sensitivity_settling: bool,
    framerate: f32,
    frame_skip: u32,
    previous: Vec<f32>,
    peaks: Vec<f32>,
    fall: Vec<f32>,
    memory: Vec<f32>,
}

impl Smoother {
    pub(super) fn new(
        bar_count: usize,
        noise_reduction: f32,
        noise_floor: f32,
        autosensitivity: u32,
    ) -> Self {
        Self {
            noise_reduction,
            noise_floor,
            autosensitivity,
            sensitivity: 1.0,
            sensitivity_initializing: true,
            sensitivity_settling: false,
            framerate: INITIAL_FRAMERATE,
            frame_skip: 1,
            previous: vec![0.0; bar_count],
            peaks: vec![0.0; bar_count],
            fall: vec![0.0; bar_count],
            memory: vec![0.0; bar_count],
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
        let integral_mod = framerate_mod.powf(0.1);
        let integral_feedback = (self.noise_reduction / integral_mod).min(MAX_INTEGRAL_FEEDBACK);
        let gravity_mod = (self.noise_reduction > 0.1)
            .then(|| framerate_mod.powf(2.5) * 2.0 / self.noise_reduction);
        // Preserve CAVA's gain search exactly, but do not expose its clipped
        // calibration frames. A cold analyzer can otherwise draw every band
        // at 1.0 while autosensitivity backs down from its first overshoot.
        let protect_initial_output = self.autosensitivity > 0
            && (self.sensitivity_initializing || self.sensitivity_settling);

        let mut overshoot = false;
        let mut max_internal = 0.0_f32;
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
            if !bar.is_finite() || *bar <= self.noise_floor {
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
                max_internal = max_internal.max(*bar);
            }
        }

        if self.autosensitivity > 0 {
            if overshoot {
                let reduction = (1.0 - 0.02 * framerate_mod).max(0.01);
                self.sensitivity *= reduction;
                self.sensitivity_initializing = false;
                self.sensitivity_settling = true;
            } else if signal_present {
                self.sensitivity_settling = false;
                self.sensitivity *= 1.0 + 0.001 * framerate_mod * self.autosensitivity as f32;
                if self.sensitivity_initializing {
                    self.sensitivity *= 1.0 + 0.1 * framerate_mod;
                }
            }
            self.sensitivity = self.sensitivity.clamp(MIN_SENSITIVITY, MAX_SENSITIVITY);
        }

        let output_scale = if self.autosensitivity > 0
            && (protect_initial_output || overshoot)
            && max_internal > INITIAL_SENSITIVITY_HEADROOM
        {
            INITIAL_SENSITIVITY_HEADROOM / max_internal
        } else {
            1.0
        };
        for bar in bars {
            *bar = (*bar * output_scale).clamp(0.0, 1.0);
        }
    }

    pub(super) fn reset(&mut self) {
        self.framerate = INITIAL_FRAMERATE;
        self.frame_skip = 1;
        self.sensitivity = 1.0;
        self.sensitivity_initializing = true;
        self.sensitivity_settling = false;
        self.previous.fill(0.0);
        self.peaks.fill(0.0);
        self.fall.fill(0.0);
        self.memory.fill(0.0);
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
mod tests {
    use super::*;

    #[test]
    fn cold_rising_signal_does_not_expose_autosensitivity_clipping() {
        let mut smoother = Smoother::new(64, 0.77, 0.04, 1);
        let mut max_mean = 0.0_f32;
        let mut max_near_full = 0;

        for raw_level in [
            0.01, 0.02, 0.04, 0.08, 0.12, 0.16, 0.18, 0.18, 0.18, 0.18, 0.18, 0.18,
        ] {
            let mut bars = [raw_level; 64];
            smoother.apply(&mut bars, 4_096, 44_100, true);
            max_mean = max_mean.max(bars.iter().sum::<f32>() / bars.len() as f32);
            max_near_full = max_near_full.max(bars.iter().filter(|bar| **bar >= 0.95).count());
        }

        assert!(
            max_mean <= INITIAL_SENSITIVITY_HEADROOM && max_near_full == 0,
            "cold rising signal saturated: max_mean={max_mean:.3}, \
             max_near_full={max_near_full}"
        );
    }

    #[test]
    fn disabled_autosensitivity_does_not_apply_initial_headroom() {
        let mut smoother = Smoother::new(1, 0.0, 0.0, 0);
        let mut bars = [1.2];

        smoother.apply(&mut bars, 735, 44_100, true);

        assert_eq!(bars, [1.0]);
    }
}
