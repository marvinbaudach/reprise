use std::time::{Duration, Instant};

const PREVIOUS_RATE_WEIGHT: u128 = 3;
const TOTAL_RATE_WEIGHT: u128 = 4;

#[derive(Debug, Default)]
pub(in crate::ui::device_sync) struct MtpRateMeter {
    bytes_per_second: u64,
    sample: Option<RateSample>,
    unit_started_at: Option<Instant>,
    nanos_per_unit: u64,
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

    pub(in crate::ui::device_sync) fn stop_copy(&mut self) {
        self.sample = None;
        self.bytes_per_second = 0;
    }

    pub(in crate::ui::device_sync) fn begin_run(&mut self, at: Instant) {
        self.unit_started_at = Some(at);
    }

    pub(in crate::ui::device_sync) fn complete_units(&mut self, count: u32, at: Instant) {
        if count == 0 {
            return;
        }
        let Some(started_at) = self.unit_started_at.replace(at) else {
            self.unit_started_at = Some(at);
            return;
        };
        let sample = duration_nanos(at.saturating_duration_since(started_at)) / u64::from(count);
        if sample == 0 {
            return;
        }
        self.nanos_per_unit = if self.nanos_per_unit == 0 {
            sample
        } else {
            smooth(self.nanos_per_unit, sample)
        };
    }

    pub(in crate::ui::device_sync) fn remaining(&self, done: u32, total: u32) -> Option<Duration> {
        let remaining = total.checked_sub(done)?;
        (remaining > 0 && self.nanos_per_unit > 0)
            .then(|| Duration::from_nanos(self.nanos_per_unit.saturating_mul(u64::from(remaining))))
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

fn duration_nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
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

#[cfg(test)]
mod unit_eta_tests {
    use super::*;

    #[test]
    fn unit_throughput_estimates_remaining_without_copy_progress() {
        let started = Instant::now();
        let mut meter = MtpRateMeter::default();
        meter.begin_run(started);

        meter.complete_units(1, started + Duration::from_secs(2));

        assert_eq!(meter.bytes_per_second(), 0);
        assert_eq!(meter.remaining(1, 3), Some(Duration::from_secs(4)));
    }
}
