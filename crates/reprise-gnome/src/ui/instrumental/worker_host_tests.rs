//! Process-boundary tests for the GTK worker supervisor. Helpers are inert
//! shell processes; no model, audio file, user database, or network is used.

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::*;

fn wait_until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !predicate() {
        assert!(
            Instant::now() < deadline,
            "condition did not become true before deadline"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn packaged_command_shares_database_and_staging_and_drains_once() {
    let spec = WorkerCommandSpec::for_paths(
        PathBuf::from("/app/libexec/reprise-worker"),
        Path::new("/data/reprise.db"),
        Path::new("/data/staging"),
    );

    assert_eq!(
        spec.executable,
        PathBuf::from("/app/libexec/reprise-worker")
    );
    assert_eq!(
        spec.args,
        [
            "--db",
            "/data/reprise.db",
            "--staging-dir",
            "/data/staging",
            "jobs",
            "work",
            "--once",
            "--lease",
            "120",
        ]
        .map(OsString::from)
    );
}

#[test]
fn idle_supervisor_starts_only_when_woken_and_serializes_runs() {
    let dir = tempfile::tempdir().unwrap();
    let count_path = dir.path().join("starts");
    let spec = WorkerCommandSpec {
        executable: PathBuf::from("/bin/sh"),
        args: vec![
            OsString::from("-c"),
            OsString::from("printf x >> \"$1\""),
            OsString::from("reprise-worker-test"),
            count_path.as_os_str().to_owned(),
        ],
    };
    let worker = InstrumentalWorker::start(spec).unwrap();

    std::thread::sleep(Duration::from_millis(50));
    assert!(
        !count_path.exists(),
        "constructing the supervisor must not load the model"
    );
    worker.wake();
    wait_until(|| {
        worker.is_idle() && std::fs::read(&count_path).is_ok_and(|bytes| bytes.len() == 1)
    });
    worker.wake();
    wait_until(|| {
        worker.is_idle() && std::fs::read(&count_path).is_ok_and(|bytes| bytes.len() == 2)
    });

    worker.shutdown();
}

#[test]
fn process_lifecycle_emits_coalesced_refresh_ticks() {
    let spec = WorkerCommandSpec {
        executable: PathBuf::from("/bin/true"),
        args: Vec::new(),
    };
    let worker = InstrumentalWorker::start(spec).unwrap();
    let receiver = worker.progress_receiver();

    worker.wake();
    wait_until(|| receiver.try_recv().is_ok());
    wait_until(|| worker.is_idle());
    worker.shutdown();
}
