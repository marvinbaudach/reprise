//! Pure podcast refresh scheduling policy.

use super::config::DEFAULT_REFRESH_HOURS;

const SECONDS_PER_HOUR: i64 = 60 * 60;
const MAX_JITTER_SECONDS: i64 = SECONDS_PER_HOUR;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RefreshRetry {
    attempt: u32,
    retry_at: i64,
}

impl RefreshRetry {
    #[must_use]
    pub const fn attempt(self) -> u32 {
        self.attempt
    }

    #[must_use]
    pub const fn retry_at(self) -> i64 {
        self.retry_at
    }

    #[must_use]
    pub const fn is_due(self, now: i64) -> bool {
        now >= self.retry_at
    }
}

/// Computes the next bounded background retry without reading a clock.
#[must_use]
pub fn next_retry(
    error: &super::PodcastError,
    previous_attempt: u32,
    failed_at: i64,
) -> Option<RefreshRetry> {
    let attempt = previous_attempt.saturating_add(1);
    let delay = error.retry_delay(attempt)?;
    let delay_seconds = i64::try_from(delay.as_secs()).unwrap_or(i64::MAX);
    Some(RefreshRetry {
        attempt,
        retry_at: failed_at.saturating_add(delay_seconds),
    })
}

#[must_use]
pub fn refresh_due(last_fetch_at: Option<i64>, now: i64, jitter: i64) -> bool {
    refresh_due_with_hours(last_fetch_at, now, DEFAULT_REFRESH_HOURS, jitter)
}

#[must_use]
pub fn refresh_due_with_hours(
    last_fetch_at: Option<i64>,
    now: i64,
    refresh_hours: i64,
    jitter: i64,
) -> bool {
    let Some(last_fetch_at) = last_fetch_at else {
        return true;
    };
    let elapsed = now.saturating_sub(last_fetch_at);
    if elapsed < 0 {
        return false;
    }
    let interval = refresh_hours
        .clamp(1, 24)
        .saturating_mul(SECONDS_PER_HOUR)
        .saturating_add(jitter.clamp(0, MAX_JITTER_SECONDS));
    elapsed >= interval
}

#[must_use]
pub fn jitter_seconds(seed: &str) -> i64 {
    let hash = crate::artist_news_refresh::fnv1a_64(seed.as_bytes());
    (hash % (MAX_JITTER_SECONDS as u64 + 1)) as i64
}

#[must_use]
pub fn should_auto_refresh(
    enabled: bool,
    subscription_count: usize,
    metered: bool,
    due: bool,
) -> bool {
    enabled && subscription_count > 0 && !metered && due
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_due_uses_interval_and_clamped_jitter() {
        let now = 100_000;
        assert!(refresh_due(None, now, 3_600));
        assert!(!refresh_due(Some(now - 21_599), now, 0));
        assert!(refresh_due(Some(now - 21_600), now, 0));
        assert!(!refresh_due(Some(now - 25_199), now, 3_600));
        assert!(refresh_due(Some(now - 25_200), now, 3_600));
        assert!(!refresh_due(Some(now + 1), now, 0));
    }

    #[test]
    fn deterministic_jitter_is_stable_and_bounded() {
        let first = jitter_seconds("/data/reprise.db");
        assert_eq!(first, jitter_seconds("/data/reprise.db"));
        assert!((0..=3_600).contains(&first));
        assert_ne!(first, jitter_seconds("/other/reprise.db"));
    }

    #[test]
    fn automatic_refresh_requires_every_gate() {
        let cases = [
            ((true, 1, false, true), true),
            ((false, 1, false, true), false),
            ((true, 0, false, true), false),
            ((true, 1, true, true), false),
            ((true, 1, false, false), false),
        ];
        for ((enabled, count, metered, due), expected) in cases {
            assert_eq!(should_auto_refresh(enabled, count, metered, due), expected);
        }
    }

    #[test]
    fn net_3d_retry_schedule_is_pure_and_uses_the_shared_backoff() {
        let failure = super::super::PodcastError::Transport("reset".to_owned());

        let retry = next_retry(&failure, 0, 1_000).unwrap();

        assert_eq!(retry.attempt(), 1);
        assert_eq!(retry.retry_at(), 1_002);
        assert!(!retry.is_due(1_001));
        assert!(retry.is_due(1_002));
    }

    #[test]
    fn net_3d_retry_schedule_caps_attempts_and_includes_typed_bot_checks() {
        let transport = super::super::PodcastError::Transport("reset".to_owned());
        let first = next_retry(&transport, 0, 1_000).unwrap();
        let second = next_retry(&transport, first.attempt(), first.retry_at()).unwrap();
        let third = next_retry(&transport, second.attempt(), second.retry_at()).unwrap();

        assert_eq!(
            [first.retry_at(), second.retry_at(), third.retry_at()],
            [1_002, 1_006, 1_014]
        );
        assert_eq!(next_retry(&transport, third.attempt(), 1_014), None);
        assert_eq!(next_retry(&transport, u32::MAX, 1_014), None);

        let bot_check = super::super::PodcastError::YtDlpFailure {
            kind: super::super::ytdlp::YtDlpFailureKind::VerificationRequired,
            stderr: "provider verification response".to_owned(),
        };
        assert_eq!(next_retry(&bot_check, 0, 2_000).unwrap().retry_at(), 2_002);
    }
}
