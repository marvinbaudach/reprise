const INITIAL_FRAMERATE: f32 = 75.0;
const CAVA_REFERENCE_FRAMERATE: f32 = 66.0;
const FALL_STEP: f32 = 0.028;

pub(super) struct Smoother {
    noise_reduction: f32,
    framerate: f32,
    frame_skip: u32,
    previous: Vec<f32>,
    peaks: Vec<f32>,
    fall: Vec<f32>,
    memory: Vec<f32>,
}

impl Smoother {
    pub(super) fn new(bar_count: usize, noise_reduction: f32) -> Self {
        Self {
            noise_reduction,
            framerate: INITIAL_FRAMERATE,
            frame_skip: 1,
            previous: vec![0.0; bar_count],
            peaks: vec![0.0; bar_count],
            fall: vec![0.0; bar_count],
            memory: vec![0.0; bar_count],
        }
    }

    pub(super) fn apply(&mut self, bars: &mut [f32], new_samples: usize, sample_rate_hz: u32) {
        self.update_framerate(new_samples, sample_rate_hz);
        let framerate_mod = CAVA_REFERENCE_FRAMERATE / self.framerate;
        let integral_mod = framerate_mod.powf(0.1);
        let gravity_mod = (self.noise_reduction > 0.1)
            .then(|| framerate_mod.powf(2.5) * 2.0 / self.noise_reduction);

        for (bar, (((previous, peak), fall), memory)) in bars.iter_mut().zip(
            self.previous
                .iter_mut()
                .zip(self.peaks.iter_mut())
                .zip(self.fall.iter_mut())
                .zip(self.memory.iter_mut()),
        ) {
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
            *bar += *memory * self.noise_reduction / integral_mod;
            *memory = *bar;
        }
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
