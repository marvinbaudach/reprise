use super::*;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[test]
fn every_key_keeps_its_interval() {
    assert_eq!(
        RateLimitKey::MusicBrainz.interval(),
        Duration::from_millis(1_000)
    );
    assert_eq!(
        RateLimitKey::AcoustId.interval(),
        Duration::from_millis(334)
    );
    assert_eq!(
        RateLimitKey::Podcasts.interval(),
        Duration::from_millis(1_000)
    );
    assert_eq!(RateLimitKey::Radio.interval(), Duration::from_millis(1_000));
    assert_eq!(
        RateLimitKey::Concerts.interval(),
        Duration::from_millis(1_000)
    );
    assert_eq!(RateLimitKey::Lrclib.interval(), Duration::from_millis(250));
    assert_eq!(RateLimitKey::Netease.interval(), Duration::from_millis(250));
    assert_eq!(RateLimitKey::Deezer.interval(), Duration::from_millis(300));
}

#[test]
fn every_key_owns_a_distinct_slot() {
    let keys = [
        RateLimitKey::MusicBrainz,
        RateLimitKey::AcoustId,
        RateLimitKey::Podcasts,
        RateLimitKey::Radio,
        RateLimitKey::Concerts,
        RateLimitKey::Lrclib,
        RateLimitKey::Netease,
        RateLimitKey::Deezer,
    ];
    assert_eq!(keys.len(), KEY_COUNT);
    let mut indexes = keys.map(RateLimitKey::index).to_vec();
    indexes.sort_unstable();
    indexes.dedup();
    assert_eq!(indexes, (0..KEY_COUNT).collect::<Vec<_>>());
}

#[test]
fn request_delay_matches_service_contracts() {
    let now = Instant::now();
    assert_eq!(
        request_delay(
            Some(now - Duration::from_millis(250)),
            now,
            RateLimitKey::MusicBrainz.interval()
        ),
        Duration::from_millis(750)
    );
    assert_eq!(
        request_delay(
            Some(now - Duration::from_millis(100)),
            now,
            RateLimitKey::AcoustId.interval()
        ),
        Duration::from_millis(234)
    );
}

#[test]
fn request_delay_enforces_one_second_interval() {
    let now = Instant::now();
    let interval = RateLimitKey::MusicBrainz.interval();
    assert_eq!(request_delay(None, now, interval), Duration::ZERO);
    assert_eq!(
        request_delay(Some(now - Duration::from_secs(2)), now, interval),
        Duration::ZERO
    );
}

#[test]
fn reserve_spaces_three_concurrent_slots_monotonically() {
    let now = Instant::now();
    let interval = RateLimitKey::AcoustId.interval();
    let mut slot = None;
    let delays = (0..3)
        .map(|_| reserve(&mut slot, now, interval))
        .collect::<Vec<_>>();
    assert_eq!(
        delays,
        [
            Duration::ZERO,
            Duration::from_millis(334),
            Duration::from_millis(668)
        ]
    );
}

#[test]
fn reserve_after_the_interval_elapsed_waits_nothing_and_records_now() {
    let now = Instant::now();
    let mut slot = Some(now - Duration::from_secs(2));
    let delay = reserve(&mut slot, now, Duration::from_secs(1));
    assert_eq!(delay, Duration::ZERO);
    assert_eq!(slot, Some(now));
}

#[test]
fn fetch_respects_rate_limit() {
    let now = Instant::now();
    let mut slot = Some(now - Duration::from_millis(250));
    let delay = reserve(&mut slot, now, RateLimitKey::MusicBrainz.interval());
    assert_eq!(delay, Duration::from_millis(750));
    assert_eq!(slot, Some(now + Duration::from_millis(750)));
}

#[test]
fn cancelled_wait_restores_the_previous_slot() {
    let t0 = Instant::now();
    let slot = Mutex::new(Some(t0));
    let mut polls = 0;
    let acquired = wait_on(&slot, Duration::from_secs(1), &mut || {
        polls += 1;
        true
    });
    assert!(!acquired);
    assert_eq!(polls, 1);
    assert_eq!(*lock_unpoisoned(&slot), Some(t0));
}

#[test]
fn cancelled_wait_keeps_a_reservation_made_after_it() {
    let slot = Mutex::new(None);
    let acquired = wait_on(&slot, Duration::from_secs(1), &mut || {
        // A second caller reserves while the first is still waiting.
        reserve(
            &mut lock_unpoisoned(&slot),
            Instant::now(),
            Duration::from_secs(1),
        );
        true
    });
    assert!(!acquired);
    assert!(lock_unpoisoned(&slot).is_some());
}

#[test]
fn uncontested_wait_acquires_without_sleeping() {
    let slot = Mutex::new(None);
    assert!(wait_on(&slot, Duration::from_secs(1), &mut || false));
    assert!(lock_unpoisoned(&slot).is_some());
}

#[test]
fn poisoned_slot_mutex_is_recovered() {
    let slot = Mutex::new(Some(Instant::now() - Duration::from_secs(5)));
    let _ = std::panic::catch_unwind(|| {
        let _guard = slot.lock().unwrap();
        panic!("poison test mutex");
    });
    assert!(slot.is_poisoned());
    assert_eq!(
        reserve(
            &mut lock_unpoisoned(&slot),
            Instant::now(),
            Duration::from_secs(1)
        ),
        Duration::ZERO
    );
}
