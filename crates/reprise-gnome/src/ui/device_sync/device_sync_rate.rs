use std::time::{Duration, Instant};

const PREVIOUS_RATE_WEIGHT: u128 = 3;
const TOTAL_RATE_WEIGHT: u128 = 4;

#[derive(Debug, Default)]
pub(in crate::ui::device_sync) struct MtpRateMeter {
    bytes_per_second: u64,
    sample: Option<RateSample>,
}

#[derive(Clone, Copy, Debug)]
struct RateSample {
    bytes: u64,
    at: Instant,
}

impl MtpRateMeter {
    pub(in crate::ui::device_sync) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(in crate::ui::device_sync) fn begin_copy(&mut self, at: Instant) {
        self.sample = Some(RateSample { bytes: 0, at });
    }

    pub(in crate::ui::device_sync) fn observe(&mut self, bytes: u64, at: Instant) -> u64 {
        let Some(previous) = self.sample else {
            self.begin_copy(at);
            return self.bytes_per_second;
        };
        if bytes <= previous.bytes || at <= previous.at {
            return self.bytes_per_second;
        }
        self.sample = Some(RateSample { bytes, at });
        let sample_rate = rate_per_second(bytes - previous.bytes, at - previous.at);
        if sample_rate == 0 {
            return self.bytes_per_second;
        }
        self.bytes_per_second = if self.bytes_per_second == 0 {
            sample_rate
        } else {
            smooth(self.bytes_per_second, sample_rate)
        };
        self.bytes_per_second
    }

    pub(in crate::ui::device_sync) fn bytes_per_second(&self) -> u64 {
        self.bytes_per_second
    }
}

fn rate_per_second(bytes: u64, elapsed: Duration) -> u64 {
    let nanos = elapsed.as_nanos();
    if nanos == 0 {
        return 0;
    }
    let rate = u128::from(bytes).saturating_mul(1_000_000_000) / nanos;
    u64::try_from(rate).unwrap_or(u64::MAX)
}

fn smooth(previous: u64, sample: u64) -> u64 {
    let weighted = u128::from(previous)
        .saturating_mul(PREVIOUS_RATE_WEIGHT)
        .saturating_add(u128::from(sample));
    u64::try_from(weighted / TOTAL_RATE_WEIGHT).unwrap_or(u64::MAX)
}
