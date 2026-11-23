//! START-6: the first instance deletes the run logs an earlier version wrote,
//! a second instance touches no file. Every test runs on its own temporary
//! state root and its own deferred log, never the process-wide one.

use std::fs;
use std::path::Path;
use std::sync::Mutex;

use reprise_core::run_log::DeferredRunLog;
use tempfile::TempDir;

use super::file_log::{discard_log, start_log};

const LEGACY_LOGS: [&str; 2] = ["reprise.log", "reprise.log.1"];

fn state_directory(root: &Path) -> std::path::PathBuf {
    root.join("reprise")
}

fn root_with_legacy_logs_and_a_bystander() -> TempDir {
    let root = TempDir::new().unwrap();
    let directory = state_directory(root.path());
    fs::create_dir_all(&directory).unwrap();
    for name in LEGACY_LOGS {
        fs::write(directory.join(name), "an old line\n").unwrap();
    }
    fs::write(directory.join("settings.keep"), "not a log\n").unwrap();
    root
}

#[test]
fn the_first_instance_removes_the_legacy_logs_and_nothing_else() {
    let root = root_with_legacy_logs_and_a_bystander();
    let log = Mutex::new(DeferredRunLog::default());

    start_log(Some(root.path()), &log);

    let directory = state_directory(root.path());
    for name in LEGACY_LOGS {
        assert!(!directory.join(name).exists(), "{name} is still there");
    }
    assert!(directory.join("settings.keep").exists());
    assert!(directory.join("run.log").exists(), "the new log is begun");
    assert!(log.lock().unwrap().is_active());
}

#[test]
fn a_second_instance_leaves_the_legacy_logs_alone() {
    let root = root_with_legacy_logs_and_a_bystander();
    let log = Mutex::new(DeferredRunLog::default());

    discard_log(&log);

    let directory = state_directory(root.path());
    for name in LEGACY_LOGS {
        assert!(directory.join(name).exists(), "{name} was removed");
    }
    assert!(!directory.join("run.log").exists());
    assert!(!log.lock().unwrap().is_active());
}

#[test]
fn without_a_state_root_the_log_stays_off() {
    let log = Mutex::new(DeferredRunLog::default());

    start_log(None, &log);

    assert!(!log.lock().unwrap().is_active());
}
