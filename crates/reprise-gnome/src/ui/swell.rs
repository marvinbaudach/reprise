const SWELL_TAU_S: f64 = 2.5;
const BREATH_PERIOD_S: f64 = 5.5;
const BREATH_FLOOR: f64 = 0.70;
const BREATH_SWING: f64 = 0.30;

/// Slow UI-side envelope for large reactive-light surfaces.
///
/// It deliberately derives only from pressure. The free-running breath is not
/// tempo-locked, so a large surface never turns a single hit into movement.
#[derive(Default)]
pub(in crate::ui) struct Swell {
    base: f64,
    elapsed_s: f64,
}

impl Swell {
    pub(in crate::ui) fn advance(&mut self, pressure: f64, dt_s: f64) {
        let pressure = if pressure.is_finite() {
            pressure.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let dt_s = if dt_s.is_finite() { dt_s.max(0.0) } else { 0.0 };
        let follow = 1.0 - (-dt_s / SWELL_TAU_S).exp();
        self.base += (pressure - self.base) * follow;
        self.elapsed_s = (self.elapsed_s + dt_s).rem_euclid(BREATH_PERIOD_S);
    }

    #[cfg(test)]
    fn base(&self) -> f64 {
        self.base
    }

    pub(in crate::ui) fn value(&self) -> f64 {
        let phase = std::f64::consts::TAU * self.elapsed_s / BREATH_PERIOD_S;
        let breath = 0.5 + 0.5 * phase.sin();
        self.base * (BREATH_FLOOR + BREATH_SWING * breath)
    }

    pub(in crate::ui) fn value_without_motion(&self) -> f64 {
        self.base
    }
}

#[cfg(test)]
mod tests {
    use super::Swell;

    #[test]
    fn ac_24_swell_follows_pressure_over_seconds_not_beats() {
        let mut s = Swell::default();
        // A step in pressure must not arrive at once: after one frame at 60 Hz
        // barely anything has moved.
        s.advance(1.0, 1.0 / 60.0);
        assert!(s.base() < 0.01, "swell jumped: {}", s.base());
        // After one time constant it is within a few percent of 1 - 1/e.
        let mut s = Swell::default();
        for _ in 0..150 {
            s.advance(1.0, 2.5 / 150.0);
        }
        assert!((s.base() - 0.632).abs() < 0.02, "tau is off: {}", s.base());
    }

    #[test]
    fn ac_24_swell_keeps_breathing_at_a_constant_pressure() {
        let mut s = Swell::default();
        for _ in 0..2_000 {
            s.advance(1.0, 0.01);
        }
        // Base has settled; the value still moves, between 0.70 and 1.00 of it.
        let mut lo = f64::MAX;
        let mut hi = f64::MIN;
        for _ in 0..600 {
            s.advance(1.0, 5.5 / 600.0);
            lo = lo.min(s.value());
            hi = hi.max(s.value());
        }
        assert!(hi - lo > 0.2, "the breath died: {lo}..{hi}");
        assert!(lo >= 0.69 && hi <= 1.01, "out of band: {lo}..{hi}");
    }

    #[test]
    fn ac_24_swell_dies_with_the_signal() {
        let mut s = Swell::default();
        for _ in 0..2_000 {
            s.advance(1.0, 0.01);
        }
        for _ in 0..4_000 {
            s.advance(0.0, 0.01);
        }
        assert!(s.value() < 0.01, "swell outlived its signal: {}", s.value());
    }

    #[test]
    fn ac_24_swell_without_animations_is_the_bare_base() {
        let mut s = Swell::default();
        for _ in 0..2_000 {
            s.advance(1.0, 0.01);
        }
        // MOT-7: the brightness stays, the movement goes.
        assert!((s.value_without_motion() - s.base()).abs() < 1e-9);
    }
}
