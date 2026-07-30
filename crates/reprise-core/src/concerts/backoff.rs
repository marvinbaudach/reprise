use std::time::Duration;

pub fn backoff_delay(attempt: u32, retry_after: Option<u64>) -> Option<Duration> {
    crate::source_error::source_backoff_delay(attempt, retry_after.map(Duration::from_secs))
}

#[cfg(test)]
#[path = "backoff_tests.rs"]
mod tests;
