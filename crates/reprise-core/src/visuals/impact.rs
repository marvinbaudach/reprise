//! Short full-canvas flash for exceptional dynamics changes.
//!
//! Beat motion belongs entirely to the radial membrane. This overlay therefore
//! tracks only a detected drop/slam and cannot keep the Grid tick loop alive
//! with invisible secondary animation state.

/// Per-frame decay of the dynamics flash.
const FLASH_DECAY: f32 = 0.90;
/// Below this the envelope is treated as fully rested.
const REST_EPSILON: f32 = 0.01;
/// `dynamics` above this reads as a drop/slam and flashes.
const DROP_THRESHOLD: f32 = 0.35;

pub(super) struct ImpactState {
    flash: f32,
}

impl Default for ImpactState {
    fn default() -> Self {
        Self::new()
    }
}

impl ImpactState {
    pub(super) fn new() -> Self {
        Self { flash: 0.0 }
    }

    /// A drop/slam adds a soft full-canvas glow. Ordinary loud passages below
    /// the threshold remain a no-op.
    pub(super) fn spawn_drop(&mut self, dynamics: f32) {
        if dynamics <= DROP_THRESHOLD {
            return;
        }
        let intensity = (dynamics - DROP_THRESHOLD) / (1.0 - DROP_THRESHOLD);
        self.flash = self.flash.max(intensity);
    }

    pub(super) fn advance(&mut self) {
        self.flash *= FLASH_DECAY;
        if self.flash < REST_EPSILON {
            self.flash = 0.0;
        }
    }

    pub(super) fn is_idle(&self) -> bool {
        self.flash == 0.0
    }

    pub(super) fn flash(&self) -> f32 {
        self.flash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_below_threshold_is_a_noop() {
        let mut impact = ImpactState::new();
        impact.spawn_drop(0.1);
        assert!(impact.is_idle(), "ordinary loudness must not flash");
        assert_eq!(impact.flash(), 0.0);
    }

    #[test]
    fn drop_flash_rises_and_decays_to_rest() {
        let mut impact = ImpactState::new();
        impact.spawn_drop(0.95);
        assert!(!impact.is_idle());
        assert!(impact.flash() > 0.0);
        for _ in 0..200 {
            impact.advance();
        }
        assert!(impact.is_idle());
        assert_eq!(impact.flash(), 0.0);
    }
}
