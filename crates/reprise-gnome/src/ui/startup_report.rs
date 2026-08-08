use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde::Serialize;

const REPORT_ENV: &str = "REPRISE_PERF_STARTUP_REPORT";
const SCHEMA_VERSION: u32 = 1;

static REPORT_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

#[derive(Debug, Serialize)]
struct Phase {
    phase: &'static str,
    offset_ms: u64,
}

#[derive(Debug, Default)]
struct StartupReport {
    phases: Vec<Phase>,
    counters: BTreeMap<String, u64>,
}

#[derive(Serialize)]
struct SerializableReport<'a> {
    schema_version: u32,
    phases: &'a [Phase],
    counters: &'a BTreeMap<String, u64>,
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

pub(crate) fn count(counter: &'static str) {
    if !is_armed() {
        return;
    }
    REPORT.with_borrow_mut(|report| {
        if let Some(value) = report.counters.get_mut(counter) {
            *value = value.saturating_add(1);
        } else {
            report.counters.insert(counter.to_owned(), 1);
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
            counters: &report.counters,
        };
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&report_path)
            .map(BufWriter::new)
            .map_err(serde_json::Error::io)
            .and_then(|writer| serde_json::to_writer(writer, &serializable))
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
        count("track_list_reload");
        count("track_list_reload");
        mark("second");
        write_if_armed();

        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(report["schema_version"], 1);
        assert_eq!(report["phases"][0]["phase"], "first");
        assert_eq!(report["phases"][1]["phase"], "second");
        assert!(
            report["phases"][0]["offset_ms"].as_u64().unwrap()
                <= report["phases"][1]["offset_ms"].as_u64().unwrap()
        );
        assert_eq!(report["counters"]["track_list_reload"], 2);
    }

    #[test]
    fn disarmed_report_records_nothing_and_writes_no_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("startup.json");
        arm(None);

        mark("ignored");
        count("track_list_reload");
        write_if_armed();

        assert!(!path.exists());
        REPORT.with_borrow(|report| {
            assert!(report.phases.is_empty());
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
        count("sidebar_rebuild");
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
