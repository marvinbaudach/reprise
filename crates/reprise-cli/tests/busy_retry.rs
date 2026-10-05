mod common;

use std::io::{BufRead, BufReader, Read};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use common::{code, parse_json, Harness};

/// The first line the CLI prints when its write layer retries a busy database.
const RETRY_NOTE: &str = "note: database busy, retrying";
/// Upper bound for every wait on another thread; a hang fails instead of stalling.
const WAIT_LIMIT: Duration = Duration::from_secs(30);

/// The holder does not sleep for a guessed duration: it commits when the CLI
/// reports its first retry. That makes the retry layer provably run (the old
/// version could pass without ever retrying) and releases the lock at the same
/// moment, whatever the host's speed.
#[test]
fn create_waits_out_a_foreign_write_transaction_by_retrying() {
    let h = Harness::new();

    let (locked_tx, locked_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let holder = {
        let fixture = h.fixture_connection();
        thread::spawn(move || {
            // BEGIN IMMEDIATE + a write takes the single WAL writer slot.
            fixture
                .execute_batch("BEGIN IMMEDIATE")
                .expect("begin immediate");
            fixture
                .execute(
                    "INSERT INTO settings (key, value) VALUES ('busy_probe', '1')",
                    [],
                )
                .expect("foreign write");
            locked_tx.send(()).expect("announce the held lock");
            // Commit on release; a dropped sender (test failed) also releases.
            let _ = release_rx.recv_timeout(WAIT_LIMIT);
            fixture.execute_batch("COMMIT").expect("commit");
        })
    };
    locked_rx
        .recv_timeout(WAIT_LIMIT)
        .expect("the holder took the write lock");

    // Only now does the CLI contend, so it can never win the lock first.
    let mut child = h.spawn_captured(&["--json", "playlist", "create", "Contended"]);
    let stderr = child.stderr.take().expect("piped stderr");
    let (retry_tx, retry_rx) = mpsc::channel();
    let stderr_reader = thread::spawn(move || {
        let mut lines = Vec::new();
        let mut announced = false;
        for line in BufReader::new(stderr).lines() {
            let line = line.expect("read CLI stderr");
            if !announced && line.starts_with(RETRY_NOTE) {
                announced = true;
                let _ = retry_tx.send(());
            }
            lines.push(line);
        }
        lines
    });

    let retried = retry_rx.recv_timeout(WAIT_LIMIT);
    release_tx
        .send(())
        .expect("release the foreign transaction");
    let status = child.wait().expect("wait for the CLI");
    let mut stdout = String::new();
    child
        .stdout
        .take()
        .expect("piped stdout")
        .read_to_string(&mut stdout)
        .expect("read CLI stdout");
    let stderr_lines = stderr_reader.join().expect("stderr reader");
    holder.join().expect("holder thread");

    assert!(
        retried.is_ok(),
        "the CLI never announced a retry; stderr was {stderr_lines:?}"
    );
    assert_eq!(
        status.code(),
        Some(0),
        "the CLI write must wait out the foreign txn, not fail busy: {stderr_lines:?}"
    );
    assert!(
        serde_json::from_str::<serde_json::Value>(&stdout).is_ok(),
        "stdout stays machine-readable JSON, retry notes go to stderr: {stdout:?}"
    );
    assert!(
        stderr_lines.iter().any(|line| line.starts_with(RETRY_NOTE)),
        "at least one retry line on stderr: {stderr_lines:?}"
    );

    let rows = parse_json(&h.run(&["--json", "playlist", "list"]));
    assert_eq!(
        rows.as_array().unwrap().len(),
        1,
        "the contended create landed"
    );
}

#[test]
fn concurrent_cli_writes_all_succeed() {
    let h = Harness::new();
    let h = &h;
    // Four separate CLI processes writing at once; WAL + busy_timeout + the
    // CLI's own retry serialize them so none is dropped.
    thread::scope(|s| {
        let handles: Vec<_> = (0..4)
            .map(|i| {
                s.spawn(move || {
                    let name = format!("P{i}");
                    h.run(&["playlist", "create", &name])
                })
            })
            .collect();
        for handle in handles {
            assert_eq!(code(&handle.join().expect("cli thread")), 0);
        }
    });

    let rows = parse_json(&h.run(&["--json", "playlist", "list"]));
    assert_eq!(rows.as_array().unwrap().len(), 4);
    assert_eq!(h.change_log_len(), 4, "each create logged exactly one row");
}
