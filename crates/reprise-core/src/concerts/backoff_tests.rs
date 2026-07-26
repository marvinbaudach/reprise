use std::time::Duration;

use super::backoff_delay;

#[test]
fn exponential_backoff_retries_three_times_then_stops() {
    assert_eq!(backoff_delay(1, None), Some(Duration::from_secs(2)));
    assert_eq!(backoff_delay(2, None), Some(Duration::from_secs(4)));
    assert_eq!(backoff_delay(3, None), Some(Duration::from_secs(8)));
    assert_eq!(backoff_delay(4, None), None);
}

#[test]
fn retry_after_wins_within_the_cap_and_aborts_beyond_it() {
    assert_eq!(backoff_delay(1, Some(1)), Some(Duration::from_secs(2)));
    assert_eq!(backoff_delay(1, Some(45)), Some(Duration::from_secs(45)));
    assert_eq!(backoff_delay(1, Some(61)), None);
}
