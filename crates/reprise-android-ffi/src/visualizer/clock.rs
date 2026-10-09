use std::time::{Duration, Instant};

pub(crate) trait MonotonicClock: Send + Sync {
    fn now(&self) -> Duration;
}

pub(crate) struct SystemMonotonicClock {
    started_at: Instant,
}

impl SystemMonotonicClock {
    pub(crate) fn new() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }
}

impl MonotonicClock for SystemMonotonicClock {
    fn now(&self) -> Duration {
        self.started_at.elapsed()
    }
}
