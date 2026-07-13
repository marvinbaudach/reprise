//! Shared blocking MusicBrainz HTTP boundary.
//!
//! Every MusicBrainz consumer goes through this module so the process-wide
//! one-request-per-second policy cannot accidentally diverge. Callers must
//! keep this work off the UI thread.

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const MIN_REQUEST_INTERVAL: Duration = Duration::from_secs(1);

pub const CONTACT_URL: &str = "https://github.com/marvinbaudach";

static LAST_REQUEST: Mutex<Option<Instant>> = Mutex::new(None);

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FetchError {
    #[error("MusicBrainz request timed out")]
    Timeout,
    #[error("MusicBrainz transport failed")]
    Transport,
    #[error("MusicBrainz returned HTTP status {0}")]
    HttpStatus(u16),
    #[error("MusicBrainz response body could not be read")]
    Body,
}

pub fn user_agent() -> String {
    format!("Reprise/{} ( {CONTACT_URL} )", env!("CARGO_PKG_VERSION"))
}

/// Performs a blocking, rate-limited MusicBrainz GET.
pub fn get(url: &str) -> Result<String, FetchError> {
    respect_rate_limit();
    let response = ureq::builder()
        .timeout(HTTP_TIMEOUT)
        .user_agent(&user_agent())
        .build()
        .get(url)
        .call()
        .map_err(classify_error)?;
    response.into_string().map_err(|_| FetchError::Body)
}

pub(crate) fn urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 3);
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn classify_error(error: ureq::Error) -> FetchError {
    match error {
        ureq::Error::Status(status, _) => FetchError::HttpStatus(status),
        ureq::Error::Transport(transport) => {
            let message = transport.to_string().to_ascii_lowercase();
            if message.contains("timed out") || message.contains("timeout") {
                FetchError::Timeout
            } else {
                FetchError::Transport
            }
        }
    }
}

fn request_delay(previous: Option<Instant>, now: Instant) -> Duration {
    let Some(previous) = previous else {
        return Duration::ZERO;
    };
    MIN_REQUEST_INTERVAL.saturating_sub(now.saturating_duration_since(previous))
}

fn respect_rate_limit() {
    let mut previous = lock_unpoisoned(&LAST_REQUEST);
    let delay = request_delay(*previous, Instant::now());
    if !delay.is_zero() {
        std::thread::sleep(delay);
    }
    *previous = Some(Instant::now());
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    #[test]
    fn user_agent_identifies_version_and_maintainer() {
        let value = user_agent();
        assert!(value.contains(env!("CARGO_PKG_VERSION")));
        assert!(value.contains("https://github.com/marvinbaudach"));
    }

    #[test]
    fn request_delay_enforces_one_second_interval() {
        let now = Instant::now();
        assert_eq!(request_delay(None, now), Duration::ZERO);
        assert_eq!(
            request_delay(Some(now - Duration::from_millis(250)), now),
            Duration::from_millis(750)
        );
        assert_eq!(
            request_delay(Some(now - Duration::from_secs(2)), now),
            Duration::ZERO
        );
    }

    #[test]
    fn poisoned_limiter_mutex_is_recovered() {
        let mutex = Mutex::new(7_u8);
        let _ = std::panic::catch_unwind(|| {
            let _guard = mutex.lock().unwrap();
            panic!("poison test mutex");
        });
        assert_eq!(*lock_unpoisoned(&mutex), 7);
    }
}
