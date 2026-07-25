use std::time::Duration;

const BASE_BACKOFF_SECONDS: u64 = 2;
const MAX_BACKOFF_SECONDS: u64 = 60;
const MAX_ATTEMPTS: u32 = 3;

pub fn backoff_delay(attempt: u32, retry_after: Option<u64>) -> Option<Duration> {
    if attempt == 0 || attempt > MAX_ATTEMPTS {
        return None;
    }
    if retry_after.is_some_and(|seconds| seconds > MAX_BACKOFF_SECONDS) {
        return None;
    }
    let exponential = BASE_BACKOFF_SECONDS
        .saturating_mul(1_u64 << attempt.saturating_sub(1))
        .min(MAX_BACKOFF_SECONDS);
    Some(Duration::from_secs(
        retry_after.unwrap_or_default().max(exponential),
    ))
}

#[cfg(test)]
#[path = "backoff_tests.rs"]
mod tests;
