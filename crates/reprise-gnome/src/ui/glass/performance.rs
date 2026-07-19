//! Fail-closed frame-cost contract and runtime collector for paired glass measurements.

use std::cell::{Cell, RefCell};
use std::fs::OpenOptions;
use std::io::BufWriter;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use serde::Serialize;

const MODE_ENV: &str = "REPRISE_GLASS_PERF_MODE";
const REPORT_ENV: &str = "REPRISE_GLASS_PERF_REPORT";
const WARMUP_FRAMES: usize = 10;

const MIN_FRAMES: usize = 120;
#[cfg(test)]
const P95_BUDGET_US: u64 = 20_000;
#[cfg(test)]
const SINGLE_FRAME_BUDGET_US: u64 = 50_000;
#[cfg(test)]
const OVERHEAD_BUDGET_US: u64 = 3_000;

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrameSeries {
    durations_us: Vec<u64>,
}

#[cfg(test)]
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

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PerfFailure {
    TooFewFrames,
    P95Budget,
    SingleFrameBudget,
    OverheadBudget,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PerfSummary {
    pub(crate) baseline_p95_us: u64,
    pub(crate) glass_p95_us: u64,
    pub(crate) glass_max_us: u64,
    pub(crate) overhead_p95_us: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum MeasurementMode {
    Baseline,
    Glass,
}

impl MeasurementMode {
    fn from_environment() -> Option<Self> {
        match std::env::var(MODE_ENV).ok()?.as_str() {
            "baseline" => Some(Self::Baseline),
            "glass" => Some(Self::Glass),
            _ => None,
        }
    }
}

#[derive(Debug, Serialize)]
struct RenderReport {
    schema_version: u32,
    mode: MeasurementMode,
    renderer: String,
    duration_kind: &'static str,
    samples_us: Vec<u64>,
}

impl RenderReport {
    fn new(mode: MeasurementMode, renderer: String, samples_us: Vec<u64>) -> Self {
        Self {
            schema_version: 1,
            mode,
            renderer,
            duration_kind: "before-paint-to-after-paint-wall-us",
            samples_us,
        }
    }
}

pub(super) fn suppress_backdrop_for_baseline() -> bool {
    MeasurementMode::from_environment() == Some(MeasurementMode::Baseline)
}

/// Arms a test-only, environment-selected collector after the window maps.
/// It continuously invalidates the window and measures CPU wall time between
/// GDK's before-paint and after-paint phases. Raw samples are retained so the
/// paired runner can calculate and audit the percentile contract itself.
pub(in crate::ui) fn arm(window: &adw::ApplicationWindow) {
    let (Some(mode), Ok(report_path)) = (
        MeasurementMode::from_environment(),
        std::env::var(REPORT_ENV),
    ) else {
        return;
    };
    let window = window.clone();
    glib::timeout_add_local_once(Duration::from_millis(250), move || {
        let Some(native) = window.native() else {
            tracing::error!("glass performance collector has no native window");
            return;
        };
        let Some(renderer) = native.renderer() else {
            tracing::error!("glass performance collector has no renderer");
            return;
        };
        let renderer_name = renderer.type_().name().to_string();
        let Some(frame_clock) = window.frame_clock() else {
            tracing::error!("glass performance collector has no frame clock");
            return;
        };
        let before = Rc::new(Cell::new(None));
        let samples = Rc::new(RefCell::new(Vec::with_capacity(MIN_FRAMES)));
        let frame_count = Rc::new(Cell::new(0usize));
        let finished = Rc::new(Cell::new(false));

        {
            let before = before.clone();
            let finished = finished.clone();
            frame_clock.connect_before_paint(move |_| {
                if !finished.get() {
                    before.set(Some(glib::monotonic_time()));
                }
            });
        }
        {
            let before = before.clone();
            let samples = samples.clone();
            let frame_count = frame_count.clone();
            let finished = finished.clone();
            let report_path = PathBuf::from(report_path);
            frame_clock.connect_after_paint(move |_| {
                if finished.get() {
                    return;
                }
                let current = frame_count.get().saturating_add(1);
                frame_count.set(current);
                let elapsed = before
                    .take()
                    .map(|started| glib::monotonic_time().saturating_sub(started))
                    .and_then(|value| u64::try_from(value).ok());
                if current > WARMUP_FRAMES {
                    if let Some(elapsed) = elapsed {
                        samples.borrow_mut().push(elapsed);
                    }
                }
                if samples.borrow().len() < MIN_FRAMES {
                    return;
                }
                finished.set(true);
                let samples = samples.borrow().clone();
                let report = RenderReport::new(mode, renderer_name.clone(), samples);
                let result = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&report_path)
                    .map(BufWriter::new)
                    .map_err(serde_json::Error::io)
                    .and_then(|writer| serde_json::to_writer_pretty(writer, &report));
                match result {
                    Ok(()) => tracing::info!(
                        path = %report_path.display(),
                        "glass performance report written"
                    ),
                    Err(error) => tracing::error!(
                        %error,
                        path = %report_path.display(),
                        "glass performance report failed"
                    ),
                }
            });
        }
        let finished_for_tick = finished.clone();
        window.add_tick_callback(move |window, _| {
            if finished_for_tick.get() {
                glib::ControlFlow::Break
            } else {
                window.queue_draw();
                glib::ControlFlow::Continue
            }
        });
    });
}

#[cfg(test)]
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

    #[test]
    fn runtime_report_names_the_cpu_paint_interval_and_keeps_raw_samples() {
        let report = RenderReport::new(
            MeasurementMode::Glass,
            "GskGLRenderer".into(),
            vec![1_250; MIN_FRAMES],
        );
        let value = serde_json::to_value(report).unwrap();

        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["mode"], "glass");
        assert_eq!(value["renderer"], "GskGLRenderer");
        assert_eq!(
            value["duration_kind"],
            "before-paint-to-after-paint-wall-us"
        );
        assert_eq!(value["samples_us"].as_array().unwrap().len(), MIN_FRAMES);
    }
}
