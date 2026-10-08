//! Bounds how long an adopted visualizer shape waits for a new stream to speak.

#[derive(Default)]
pub(crate) struct AdoptedShapeHold {
    phase: Phase,
}

#[derive(Default)]
enum Phase {
    #[default]
    Inactive,
    AwaitingSignal {
        silent_samples: usize,
    },
    SignalArrived,
}

impl AdoptedShapeHold {
    pub(crate) fn begin(&mut self) {
        self.phase = Phase::AwaitingSignal { silent_samples: 0 };
    }

    pub(crate) fn clear(&mut self) {
        self.phase = Phase::Inactive;
    }

    pub(crate) fn is_active(&self) -> bool {
        !matches!(self.phase, Phase::Inactive)
    }

    pub(crate) fn should_hold(
        &mut self,
        boundary_waiting: bool,
        signal_present: bool,
        analyzed_samples: usize,
        boundary_window_samples: usize,
    ) -> bool {
        if !self.is_active() || !boundary_waiting {
            self.clear();
            return false;
        }

        match &mut self.phase {
            Phase::Inactive => false,
            Phase::SignalArrived => true,
            Phase::AwaitingSignal { silent_samples } if signal_present => {
                self.phase = Phase::SignalArrived;
                true
            }
            Phase::AwaitingSignal { silent_samples } => {
                *silent_samples = silent_samples.saturating_add(analyzed_samples);
                if *silent_samples > boundary_window_samples {
                    self.clear();
                    false
                } else {
                    true
                }
            }
        }
    }
}
