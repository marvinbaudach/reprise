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

    // A track change clears the bar history (`previous`/`peaks`/`fall`/
    // `memory`) but keeps the settled autosensitivity gain
    // (`sensitivity`/`sensitivity_initializing`/`sensitivity_settling`).
    // Consecutive tracks are usually mastered to a similar loudness, so
    // re-running the cold-start calibration on every change would force a
    // visible recalibration (the `output_scale` headroom clamp, then an
    // overshoot correction once `sensitivity_settling` releases) even
    // though the previous gain was already a good estimate. `framerate`
    // and `frame_skip` are also kept: the device's output frame rate does
    // not change across a track boundary, so there is nothing to
    // recalibrate there either.
    pub(super) fn reset(&mut self) {
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
    // A track change tears down and rebuilds the visualizer engine, which
    // calls `reset()`. If autosensitivity's cold-start headroom cap (the
    // `output_scale` branch above) inflated a quiet track's bars up to the
    // same 0.85 ceiling a loud track reaches, the bars would visibly slam
    // out on every track change regardless of how quiet the new track is.
    // Control arm: the same constant quiet signal fed until autosensitivity
    // has settled (`sensitivity_initializing` and `sensitivity_settling`
    // both false — reached at frame 32 for this fixture, confirmed by
    // instrumented run). Fix arm: `reset()`, then the first frame of the
    // identical signal again.
    fn cold_start_does_not_inflate_a_quiet_signal_to_full_scale() {
        let quiet_level = 0.10_f32;
        let mut smoother = Smoother::new(64, 0.77, 0.04, 1);

        let mut settled_max = 0.0_f32;
        loop {
            let mut bars = [quiet_level; 64];
            smoother.apply(&mut bars, 4_096, 44_100, true);
            if !smoother.sensitivity_initializing && !smoother.sensitivity_settling {
                settled_max = settled_max.max(bars.iter().cloned().fold(0.0_f32, f32::max));
                break;
            }
        }
        for _ in 0..10 {
            let mut bars = [quiet_level; 64];
            smoother.apply(&mut bars, 4_096, 44_100, true);
            settled_max = settled_max.max(bars.iter().cloned().fold(0.0_f32, f32::max));
        }

        smoother.reset();
        let mut cold_bars = [quiet_level; 64];
        smoother.apply(&mut cold_bars, 4_096, 44_100, true);
        let cold_max = cold_bars.iter().cloned().fold(0.0_f32, f32::max);

        assert!(
            cold_max <= settled_max * 1.5,
            "cold-start frame drew a quiet signal far above its settled level: \
             cold_max={cold_max:.3}, settled_max={settled_max:.3}"
        );
    }

    #[test]
    // Regression test for the bug this fix addresses: `reset()` used to zero
    // `sensitivity`/`sensitivity_initializing`/`sensitivity_settling` too,
    // forcing every track change through the cold-start calibration again.
    // For a quiet track that meant the `output_scale` headroom clamp held
    // the visible maximum flat at `INITIAL_SENSITIVITY_HEADROOM` (0.85) for
    // ~25 frames, then jumped past the real plateau once
    // `sensitivity_settling` released — a visible slam on every track
    // change. With the settled gain kept across `reset()`, the same quiet
    // signal should stay near its already-known plateau the whole time.
    fn keeps_its_calibration_across_a_track_change() {
        // Real device cadence, not an arbitrary fixture: `tick()` runs in
        // `withFrameNanos`, i.e. once per display frame, and feeds
        // `apply()` with the samples it read since the last tick. On the
        // device this is 800 samples at 48_000 Hz, ~16.7 ms/frame (60 fps).
        // `update_framerate()` derives `framerate` (and thus
        // `framerate_mod`, which scales both the sensitivity ramp step and
        // the gravity term) directly from these two numbers, so a fixture
        // using a different (samples, sample_rate) pair exercises a
        // different regime than the one the bug was measured in. At 60 fps
        // vs. the old fixture's ~10.77 fps (4_096 samples @ 44_100 Hz), the
        // same elapsed time takes ~5.57x as many frames
        // (= 60 / (44_100 / 4_096)), so frame counts below are the old
        // counts scaled by that factor.
        const SAMPLES: usize = 800;
        const SAMPLE_RATE_HZ: u32 = 48_000;

        let quiet_level = 0.10_f32;
        let mut smoother = Smoother::new(64, 0.77, 0.04, 1);

        // Run past the initial cold-start transient (which legitimately
        // overshoots while autosensitivity searches, per
        // `cold_rising_signal_does_not_expose_autosensitivity_clipping`)
        // before measuring the settled plateau.
        loop {
            let mut bars = [quiet_level; 64];
            smoother.apply(&mut bars, SAMPLES, SAMPLE_RATE_HZ, true);
            if !smoother.sensitivity_initializing && !smoother.sensitivity_settling {
                break;
            }
        }
        let mut plateau_max = 0.0_f32;
        for _ in 0..223 {
            let mut bars = [quiet_level; 64];
            smoother.apply(&mut bars, SAMPLES, SAMPLE_RATE_HZ, true);
            plateau_max = plateau_max.max(bars.iter().cloned().fold(0.0_f32, f32::max));
        }

        smoother.reset();

        // The `max_consecutive_at_headroom` check below is the only assertion
        // in this test that actually discriminates the bug at this cadence:
        // `plateau_max` settles at ~0.999 here, so `plateau_max * 1.05`
        // exceeds the output's hard `.clamp(0.0, 1.0)` ceiling and the two
        // `<= plateau_max * 1.05` asserts below can never fail on their own.
        //
        // Occasionally pinning a single frame to `INITIAL_SENSITIVITY_HEADROOM`
        // is normal steady-state behaviour (the `overshoot` branch of
        // `output_scale` fires whenever one frame's internal value ticks
        // past 1.0) — at this cadence a correctly-fixed `reset()` still
        // shows short runs of up to 6 consecutive frames pinned there
        // (measured directly on this exact fixture: temporarily tightening
        // the threshold below to 0 and reading the panic message). The bug
        // this test guards against is a much longer *sustained* run: with
        // the old `reset()` behaviour (re-zeroing
        // `sensitivity`/`sensitivity_initializing`/`sensitivity_settling`)
        // temporarily reinstated, the same fixture produced a run of 26
        // consecutive frames flat at the headroom immediately after reset,
        // confirmed the same way (`cargo test ... -- --nocapture`, EXIT=101).
        // The threshold below sits well above the fixed arm's measured
        // maximum (6) and well below the bug's measured signature (26).
        let mut post_reset_max = 0.0_f32;
        let mut consecutive_at_headroom = 0;
        let mut max_consecutive_at_headroom = 0;
        for _ in 0..446 {
            let mut bars = [quiet_level; 64];
            smoother.apply(&mut bars, SAMPLES, SAMPLE_RATE_HZ, true);
            let frame_max = bars.iter().cloned().fold(0.0_f32, f32::max);
            post_reset_max = post_reset_max.max(frame_max);
            assert!(
                frame_max <= plateau_max * 1.05,
                "frame after reset overshot the already-known plateau: \
                 frame_max={frame_max:.3}, plateau_max={plateau_max:.3}"
            );
            if (frame_max - INITIAL_SENSITIVITY_HEADROOM).abs() < 0.001
                && (plateau_max - INITIAL_SENSITIVITY_HEADROOM).abs() > 0.01
            {
                consecutive_at_headroom += 1;
                max_consecutive_at_headroom =
                    max_consecutive_at_headroom.max(consecutive_at_headroom);
            } else {
                consecutive_at_headroom = 0;
            }
        }

        assert!(
            max_consecutive_at_headroom <= 12,
            "bars clamped flat at the cold-start headroom for {max_consecutive_at_headroom} \
             consecutive frames instead of tracking the known plateau \
             (plateau_max={plateau_max:.3})"
        );
        assert!(
            post_reset_max <= plateau_max * 1.05,
            "post-reset maximum exceeded the known plateau: \
             post_reset_max={post_reset_max:.3}, plateau_max={plateau_max:.3}"
        );
    }

    #[test]
    // Control arm for the fix above: a loud-to-quiet track change is a case
    // where keeping the old sensitivity is genuinely wrong for a while, and
    // that is expected. This test documents that the autosensitivity still
    // recovers and converges on the quiet signal's correct plateau within a
    // bounded number of frames, rather than sticking at the loud track's
    // gain or oscillating.
    fn recovers_after_a_loud_to_quiet_track_change() {
        // Same real device cadence and scaling rationale as
        // `keeps_its_calibration_across_a_track_change` above: 800 samples
        // @ 48_000 Hz (~60 fps, the `withFrameNanos` tick rate), frame
        // counts scaled from the old 4_096 @ 44_100 Hz fixture (~10.77 fps)
        // by ~5.57x (= 60 / (44_100 / 4_096)) to cover the same elapsed
        // time.
        const SAMPLES: usize = 800;
        const SAMPLE_RATE_HZ: u32 = 48_000;

        let loud_level = 0.80_f32;
        let quiet_level = 0.10_f32;

        // Reference: the plateau a fresh smoother settles at for the quiet
        // signal alone, with no prior loud-signal history. Measured only
        // after the cold-start transient has passed, same as the
        // regression test above.
        let mut reference = Smoother::new(64, 0.77, 0.04, 1);
        loop {
            let mut bars = [quiet_level; 64];
            reference.apply(&mut bars, SAMPLES, SAMPLE_RATE_HZ, true);
            if !reference.sensitivity_initializing && !reference.sensitivity_settling {
                break;
            }
        }
        let mut reference_plateau = 0.0_f32;
        for _ in 0..223 {
            let mut bars = [quiet_level; 64];
            reference.apply(&mut bars, SAMPLES, SAMPLE_RATE_HZ, true);
            reference_plateau = reference_plateau.max(bars.iter().cloned().fold(0.0_f32, f32::max));
        }

        let mut smoother = Smoother::new(64, 0.77, 0.04, 1);
        for _ in 0..446 {
            let mut bars = [loud_level; 64];
            smoother.apply(&mut bars, SAMPLES, SAMPLE_RATE_HZ, true);
        }

        smoother.reset();

        // Feed the quiet signal long enough for the gain to climb back up
        // (a loud-to-quiet change needs sensitivity to grow, which happens
        // in small multiplicative steps), then measure its own plateau the
        // same way as the reference.
        for _ in 0..11_147 {
            let mut bars = [quiet_level; 64];
            smoother.apply(&mut bars, SAMPLES, SAMPLE_RATE_HZ, true);
        }
        let mut converged_max = 0.0_f32;
        for _ in 0..223 {
            let mut bars = [quiet_level; 64];
            smoother.apply(&mut bars, SAMPLES, SAMPLE_RATE_HZ, true);
            converged_max = converged_max.max(bars.iter().cloned().fold(0.0_f32, f32::max));
        }

        assert!(
            (converged_max - reference_plateau).abs() <= reference_plateau * 0.15 + 0.02,
            "autosensitivity failed to converge on the quiet plateau after a \
             loud-to-quiet track change: converged_max={converged_max:.3}, \
             reference_plateau={reference_plateau:.3}"
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
