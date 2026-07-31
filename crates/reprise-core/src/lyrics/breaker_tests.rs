use super::*;

const HOST: &str = "lyrics.example";

#[test]
fn breaker_opens_after_three_failures_but_not_two() {
    let breaker = Breaker::new(3, 300);

    breaker.record(HOST, BreakerOutcome::Failure, 10);
    breaker.record(HOST, BreakerOutcome::Failure, 11);
    assert!(breaker.can_attempt(HOST, 12, false));

    breaker.record(HOST, BreakerOutcome::Failure, 12);
    assert!(!breaker.can_attempt(HOST, 13, false));
}

#[test]
fn not_found_does_not_increment_the_failure_count() {
    let breaker = Breaker::new(3, 300);

    for now in 1..=5 {
        breaker.record(HOST, BreakerOutcome::NotFound, now);
    }

    assert!(breaker.can_attempt(HOST, 6, false));
}

#[test]
fn success_resets_accumulated_failures() {
    let breaker = Breaker::new(3, 300);
    breaker.record(HOST, BreakerOutcome::Failure, 1);
    breaker.record(HOST, BreakerOutcome::Failure, 2);
    breaker.record(HOST, BreakerOutcome::Success, 3);
    breaker.record(HOST, BreakerOutcome::Failure, 4);
    breaker.record(HOST, BreakerOutcome::Failure, 5);

    assert!(breaker.can_attempt(HOST, 6, false));
}

#[test]
fn expired_open_window_allows_requests_again_without_sleeping() {
    let breaker = Breaker::new(3, 300);
    for now in 1..=3 {
        breaker.record(HOST, BreakerOutcome::Failure, now);
    }

    assert!(!breaker.can_attempt(HOST, 302, false));
    assert!(breaker.can_attempt(HOST, 303, false));
}

#[test]
fn forced_request_bypasses_an_open_breaker() {
    let breaker = Breaker::new(3, 300);
    for now in 1..=3 {
        breaker.record(HOST, BreakerOutcome::Failure, now);
    }

    assert!(!breaker.can_attempt(HOST, 4, false));
    assert!(breaker.can_attempt(HOST, 4, true));
}
