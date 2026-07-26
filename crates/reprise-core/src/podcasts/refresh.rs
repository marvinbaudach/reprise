//! Pure podcast refresh scheduling policy.

use super::config::DEFAULT_REFRESH_HOURS;

const SECONDS_PER_HOUR: i64 = 60 * 60;
const MAX_JITTER_SECONDS: i64 = SECONDS_PER_HOUR;

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
}
