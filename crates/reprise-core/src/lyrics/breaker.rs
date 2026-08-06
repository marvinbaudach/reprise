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
    retry_after_until: Option<i64>,
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
        let states = self.states();
        let Some(state) = states.get(host) else {
            return true;
        };
        if state
            .retry_after_until
            .is_some_and(|retry_after_until| now < retry_after_until)
        {
            return false;
        }
        force || state.open_until.is_none_or(|open_until| now >= open_until)
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

    pub(super) fn record_rate_limited_until(
        &self,
        host: &'static str,
        now: i64,
        retry_after_until: Option<i64>,
    ) {
        let Some(retry_after_until) = retry_after_until else {
            self.record(host, BreakerOutcome::Failure, now);
            return;
        };
        let mut states = self.states();
        let state = states.entry(host).or_default();
        state.retry_after_until = Some(
            state
                .retry_after_until
                .map_or(retry_after_until, |current| current.max(retry_after_until)),
        );
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
