use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, MutexGuard};

const FAILURE_LIMIT: u32 = 3;
const OPEN_SECONDS: i64 = 5 * 60;

pub(super) static HOST_BREAKER: LazyLock<Breaker> =
    LazyLock::new(|| Breaker::new(FAILURE_LIMIT, OPEN_SECONDS));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BreakerOutcome {
    Success,
    NotFound,
    Failure,
}

#[derive(Clone, Copy, Debug, Default)]
struct BreakerState {
    failures: u32,
    open_until: Option<i64>,
}

pub(super) struct Breaker {
    states: Mutex<HashMap<&'static str, BreakerState>>,
    failure_limit: u32,
    open_seconds: i64,
}

impl Breaker {
    pub(super) fn new(failure_limit: u32, open_seconds: i64) -> Self {
        Self {
            states: Mutex::new(HashMap::new()),
            failure_limit,
            open_seconds,
        }
    }

    pub(super) fn can_attempt(&self, host: &'static str, now: i64, force: bool) -> bool {
        if force {
            return true;
        }
        self.states()
            .get(host)
            .and_then(|state| state.open_until)
            .is_none_or(|open_until| now >= open_until)
    }

    pub(super) fn record(&self, host: &'static str, outcome: BreakerOutcome, now: i64) {
        let mut states = self.states();
        let state = states.entry(host).or_default();
        match outcome {
            BreakerOutcome::Success | BreakerOutcome::NotFound => {
                *state = BreakerState::default();
            }
            BreakerOutcome::Failure => {
                state.failures = state.failures.saturating_add(1);
                if state.failures >= self.failure_limit {
                    state.open_until = Some(now.saturating_add(self.open_seconds));
                }
            }
        }
    }

    pub(super) fn all_open(&self, hosts: &[&'static str], now: i64) -> bool {
        !hosts.is_empty() && hosts.iter().all(|host| !self.can_attempt(host, now, false))
    }

    fn states(&self) -> MutexGuard<'_, HashMap<&'static str, BreakerState>> {
        self.states
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
#[path = "breaker_tests.rs"]
mod tests;
