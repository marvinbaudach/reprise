use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Instant;

use serde::Serialize;

const REPORT_ENV: &str = "REPRISE_PERF_STARTUP_REPORT";
const SCHEMA_VERSION: u32 = 2;

static REPORT_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

#[derive(Debug, Serialize)]
struct Phase {
    phase: &'static str,
    offset_ms: u64,
}

#[derive(Debug, Serialize)]
struct Measurement {
    measurement: &'static str,
    duration_us: u64,
}

#[derive(Debug, Default)]
struct StartupReport {
    phases: Vec<Phase>,
    measurements: Vec<Measurement>,
    counters: BTreeMap<String, u64>,
}

#[derive(Serialize)]
struct SerializableReport<'a> {
    schema_version: u32,
    phases: &'a [Phase],
    measurements: &'a [Measurement],
    counters: &'a BTreeMap<String, u64>,
}

/// One env-gated startup measurement and its matching tracing span.
///
/// The normal path pays only the already-cached armed check. An armed report
/// retains microsecond precision because the single-digit-millisecond views
/// are precisely the ones B1 must distinguish from material deferral targets.
pub(crate) struct MeasurementGuard {
    measurement: &'static str,
    started_at: Option<Instant>,
    span: tracing::Span,
}

impl Drop for MeasurementGuard {
    fn drop(&mut self) {
        let Some(started_at) = self.started_at else {
            return;
        };
        let duration_us = u64::try_from(started_at.elapsed().as_micros()).unwrap_or(u64::MAX);
        REPORT.with_borrow_mut(|report| {
            report.measurements.push(Measurement {
                measurement: self.measurement,
                duration_us,
            });
        });
        let _entered = self.span.enter();
        tracing::info!(duration_us, "startup measurement complete");
    }
}

thread_local! {
    static REPORT: RefCell<StartupReport> = RefCell::new(StartupReport::default());
}

#[cfg(test)]
thread_local! {
    static REPORT_PATH_OVERRIDE: RefCell<Option<Option<PathBuf>>> = const { RefCell::new(None) };
}

fn configured_path() -> &'static Option<PathBuf> {
    REPORT_PATH.get_or_init(|| std::env::var(REPORT_ENV).ok().map(PathBuf::from))
}

fn is_armed() -> bool {
    #[cfg(test)]
    if let Some(override_path) = REPORT_PATH_OVERRIDE.with(|slot| slot.borrow().clone()) {
        return override_path.is_some();
    }

    configured_path().is_some()
}

fn report_path() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(override_path) = REPORT_PATH_OVERRIDE.with(|slot| slot.borrow().clone()) {
        return override_path;
    }

    configured_path().clone()
}

pub(crate) fn mark(phase: &'static str) -> bool {
    if !is_armed() {
        return false;
    }
    REPORT.with_borrow_mut(|report| {
        record_phase(
            report,
            phase,
            super::track_list::diagnostic_trail::process_start(),
        );
    });
    true
}

pub(crate) fn measure(measurement: &'static str) -> MeasurementGuard {
    let armed = is_armed();
    MeasurementGuard {
        measurement,
        started_at: armed.then(Instant::now),
        span: if armed {
            tracing::info_span!("startup measurement", measurement)
        } else {
            tracing::Span::none()
        },
    }
}

fn record_phase(
    report: &mut StartupReport,
    phase: &'static str,
    start: Option<std::time::Instant>,
) {
    let Some(start) = start else {
        return;
    };
    let offset_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    report.phases.push(Phase { phase, offset_ms });
}

pub(crate) fn event(name: &'static str) {
    if !is_armed() {
        return;
    }
    REPORT.with_borrow_mut(|report| {
        record_phase(
            report,
            name,
            super::track_list::diagnostic_trail::process_start(),
        );
        if let Some(value) = report.counters.get_mut(name) {
            *value = value.saturating_add(1);
        } else {
            report.counters.insert(name.to_owned(), 1);
        }
    });
}

pub(crate) fn write_if_armed() {
    let Some(report_path) = report_path() else {
        return;
    };
    let result = REPORT.with_borrow(|report| {
        let serializable = SerializableReport {
            schema_version: SCHEMA_VERSION,
            phases: &report.phases,
            measurements: &report.measurements,
            counters: &report.counters,
        };
        reprise_core::perf_report::write_new_json(&report_path, &serializable)
    });
    match result {
        Ok(()) => {
            tracing::info!(path = %report_path.display(), "startup performance report written");
        }
        Err(error) => {
            tracing::error!(%error, path = %report_path.display(), "startup performance report failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arm(path: Option<PathBuf>) {
        REPORT_PATH_OVERRIDE.with(|slot| *slot.borrow_mut() = Some(path));
        REPORT.with_borrow_mut(|report| *report = StartupReport::default());
    }

    #[test]
    fn armed_report_preserves_phase_order_and_counter_totals() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("startup.json");
        arm(Some(path.clone()));
        crate::ui::track_list::diagnostic_trail::mark_process_start();

        mark("first");
        drop(measure("view.stats.construct"));
        event("track_list_reload");
        event("track_list_reload");
        mark("second");
        write_if_armed();

        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(report["schema_version"], 2);
        assert_eq!(report["phases"][0]["phase"], "first");
        assert_eq!(report["phases"][1]["phase"], "track_list_reload");
        assert_eq!(report["phases"][2]["phase"], "track_list_reload");
        assert_eq!(report["phases"][3]["phase"], "second");
        assert!(
            report["phases"][0]["offset_ms"].as_u64().unwrap()
                <= report["phases"][3]["offset_ms"].as_u64().unwrap()
        );
        assert_eq!(report["counters"]["track_list_reload"], 2);
        assert_eq!(
            report["measurements"][0]["measurement"],
            "view.stats.construct"
        );
        assert!(report["measurements"][0]["duration_us"].is_u64());
    }

    #[test]
    fn disarmed_report_records_nothing_and_writes_no_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("startup.json");
        arm(None);

        mark("ignored");
        drop(measure("view.stats.construct"));
        event("track_list_reload");
        write_if_armed();

        assert!(!path.exists());
        REPORT.with_borrow(|report| {
            assert!(report.phases.is_empty());
            assert!(report.measurements.is_empty());
            assert!(report.counters.is_empty());
        });
    }

    #[test]
    fn unwritable_report_path_does_not_panic_or_create_a_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("missing").join("startup.json");
        arm(Some(path.clone()));
        crate::ui::track_list::diagnostic_trail::mark_process_start();

        mark("first");
        event("sidebar_rebuild");
        write_if_armed();

        assert!(!path.exists());
    }

    #[test]
    fn a_missing_process_clock_drops_the_phase_without_panicking() {
        let mut report = StartupReport::default();

        record_phase(&mut report, "cold call", None);

        assert!(report.phases.is_empty());
    }
}
