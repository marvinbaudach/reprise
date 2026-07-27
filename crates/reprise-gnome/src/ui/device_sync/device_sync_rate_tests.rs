use std::time::{Duration, Instant};

use super::device_sync_runtime::rate::MtpRateMeter;

#[test]
fn mtp_rate_is_smoothed_across_copy_samples_but_excludes_non_copy_gaps() {
    let start = Instant::now();
    let mut meter = MtpRateMeter::default();

    meter.begin_copy(start);
    assert_eq!(meter.observe(1_000, start + Duration::from_secs(1)), 1_000);
    assert_eq!(meter.observe(3_000, start + Duration::from_secs(2)), 1_250);

    meter.begin_copy(start + Duration::from_secs(62));
    assert_eq!(meter.observe(4_000, start + Duration::from_secs(63)), 1_937);
}

#[test]
fn mtp_rate_resets_between_device_sync_runs() {
    let start = Instant::now();
    let mut meter = MtpRateMeter::default();
    meter.begin_copy(start);
    meter.observe(8_000, start + Duration::from_secs(2));
    assert_eq!(meter.bytes_per_second(), 4_000);

    meter.reset();

    assert_eq!(meter.bytes_per_second(), 0);
    meter.begin_copy(start + Duration::from_secs(120));
    assert_eq!(
        meter.observe(2_000, start + Duration::from_secs(121)),
        2_000
    );
}
