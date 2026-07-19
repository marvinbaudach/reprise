//! Fail-closed frame-cost contract for paired glass measurements.

const MIN_FRAMES: usize = 120;
const P95_BUDGET_US: u64 = 20_000;
const SINGLE_FRAME_BUDGET_US: u64 = 50_000;
const OVERHEAD_BUDGET_US: u64 = 3_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrameSeries {
    durations_us: Vec<u64>,
}

impl FrameSeries {
    pub(crate) fn new(durations_us: Vec<u64>) -> Self {
        Self { durations_us }
    }

    fn p95(&self) -> Option<u64> {
        if self.durations_us.is_empty() {
            return None;
        }
        let mut sorted = self.durations_us.clone();
        sorted.sort_unstable();
        let rank = (sorted.len() * 95).div_ceil(100);
        sorted.get(rank.saturating_sub(1)).copied()
    }

    fn maximum(&self) -> Option<u64> {
        self.durations_us.iter().copied().max()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PerfFailure {
    TooFewFrames,
    P95Budget,
    SingleFrameBudget,
    OverheadBudget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PerfSummary {
    pub(crate) baseline_p95_us: u64,
    pub(crate) glass_p95_us: u64,
    pub(crate) glass_max_us: u64,
    pub(crate) overhead_p95_us: u64,
}

pub(crate) fn evaluate_pair(
    baseline: &FrameSeries,
    glass: &FrameSeries,
) -> Result<PerfSummary, PerfFailure> {
    if baseline.durations_us.len() < MIN_FRAMES || glass.durations_us.len() < MIN_FRAMES {
        return Err(PerfFailure::TooFewFrames);
    }
    let baseline_p95 = baseline.p95().ok_or(PerfFailure::TooFewFrames)?;
    let glass_p95 = glass.p95().ok_or(PerfFailure::TooFewFrames)?;
    let glass_max = glass.maximum().ok_or(PerfFailure::TooFewFrames)?;
    if glass_p95 > P95_BUDGET_US {
        return Err(PerfFailure::P95Budget);
    }
    if glass_max > SINGLE_FRAME_BUDGET_US {
        return Err(PerfFailure::SingleFrameBudget);
    }
    let overhead = glass_p95.saturating_sub(baseline_p95);
    if overhead > OVERHEAD_BUDGET_US {
        return Err(PerfFailure::OverheadBudget);
    }
    Ok(PerfSummary {
        baseline_p95_us: baseline_p95,
        glass_p95_us: glass_p95,
        glass_max_us: glass_max,
        overhead_p95_us: overhead,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_uses_nearest_rank_and_is_order_independent() {
        let mut samples: Vec<u64> = (1..=100).collect();
        samples.reverse();
        assert_eq!(FrameSeries::new(samples).p95(), Some(95));
    }

    #[test]
    fn faster_glass_never_reports_negative_overhead() {
        let summary = evaluate_pair(
            &FrameSeries::new(vec![15_000; MIN_FRAMES]),
            &FrameSeries::new(vec![14_000; MIN_FRAMES]),
        )
        .unwrap();
        assert_eq!(summary.overhead_p95_us, 0);
    }
}
