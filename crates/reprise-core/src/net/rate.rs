//! Request spacing for every metadata provider.
//!
//! One algorithm serves all of them: the next slot is reserved under a short lock and the sleep
//! happens outside it, so no lock is ever held across a wait. Slots are keyed by provider budget,
//! not by host: Cover Art Archive requests share MusicBrainz's slot, radio-browser mirrors share
//! one, and fixture-mode requests have no host at all.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::lock_unpoisoned;

/// One spacing budget per provider family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RateLimitKey {
    MusicBrainz,
    AcoustId,
    Podcasts,
    Radio,
    Concerts,
    Lrclib,
    Netease,
    Deezer,
}

impl RateLimitKey {
    /// The minimum spacing between two requests that share this key.
    pub(crate) const fn interval(self) -> Duration {
        match self {
            Self::MusicBrainz | Self::Podcasts | Self::Radio | Self::Concerts => {
                Duration::from_millis(1_000)
            }
            Self::AcoustId => Duration::from_millis(334),
            Self::Lrclib | Self::Netease => Duration::from_millis(250),
            Self::Deezer => Duration::from_millis(300),
        }
    }

    const fn index(self) -> usize {
        self as usize
    }
}

const KEY_COUNT: usize = 8;
const SLICE: Duration = Duration::from_millis(50);

// One mutex per key: no cross-provider contention, and a poisoned slot stays isolated.
static SLOTS: [Mutex<Option<Instant>>; KEY_COUNT] = [const { Mutex::new(None) }; KEY_COUNT];

fn slot(key: RateLimitKey) -> &'static Mutex<Option<Instant>> {
    &SLOTS[key.index()]
}

/// How long a request sent at `now` must wait. A future `previous` (a reservation) adds the
/// interval on top of the time until it.
pub(crate) fn request_delay(
    previous: Option<Instant>,
    now: Instant,
    interval: Duration,
) -> Duration {
    previous.map_or(Duration::ZERO, |value| {
        if value > now {
            value.duration_since(now).saturating_add(interval)
        } else {
            interval.saturating_sub(now.duration_since(value))
        }
    })
}

/// Pure core of the limiter: computes the delay from `*previous`, stores the reserved slot and
/// returns the delay.
pub(crate) fn reserve(
    previous: &mut Option<Instant>,
    now: Instant,
    interval: Duration,
) -> Duration {
    let delay = request_delay(*previous, now, interval);
    *previous = Some(now + delay);
    delay
}

/// Reserves the next slot for `key` and returns how long the caller must wait before sending.
/// The reservation stays even if the caller never sends.
pub(crate) fn reserve_slot(key: RateLimitKey) -> Duration {
    reserve(
        &mut lock_unpoisoned(slot(key)),
        Instant::now(),
        key.interval(),
    )
}

/// Reserves, then sleeps in 50 ms slices, polling `cancelled()` before every slice and once more
/// after the last one. Returns `false` on cancellation; a cancelled wait gives its reservation
/// back unless a later caller has reserved behind it, so a cancelled request records nothing.
pub(crate) fn wait_for_slot(key: RateLimitKey, cancelled: &mut dyn FnMut() -> bool) -> bool {
    wait_on(slot(key), key.interval(), cancelled)
}

fn wait_on(
    slot: &Mutex<Option<Instant>>,
    interval: Duration,
    cancelled: &mut dyn FnMut() -> bool,
) -> bool {
    let (previous, reserved, mut delay) = {
        let mut guard = lock_unpoisoned(slot);
        let previous = *guard;
        let delay = reserve(&mut guard, Instant::now(), interval);
        (previous, *guard, delay)
    };
    while !delay.is_zero() {
        if cancelled() {
            give_back(slot, previous, reserved);
            return false;
        }
        let slice = delay.min(SLICE);
        std::thread::sleep(slice);
        delay = delay.saturating_sub(slice);
    }
    if cancelled() {
        give_back(slot, previous, reserved);
        return false;
    }
    true
}

fn give_back(slot: &Mutex<Option<Instant>>, previous: Option<Instant>, reserved: Option<Instant>) {
    let mut guard = lock_unpoisoned(slot);
    if *guard == reserved {
        *guard = previous;
    }
}

#[cfg(test)]
#[path = "rate_tests.rs"]
mod tests;
