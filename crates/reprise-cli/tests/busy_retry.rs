mod common;

use std::thread;
use std::time::Duration;

use common::{code, parse_json, Harness};

/// How long the foreign transaction holds the write lock.
const HOLD_MS: u64 = 400;
/// How long to wait before contending, so the holder has the lock first.
const SETTLE_MS: u64 = 150;

#[test]
fn create_waits_out_a_briefly_held_foreign_write_transaction() {
    let h = Harness::new();

    thread::scope(|s| {
        let db = &h.db;
        let holder = s.spawn(move || {
            let conn =
                reprise_core::db::open_migrated(Some(db.as_path())).expect("holder connection");
            // BEGIN IMMEDIATE + a write takes the single WAL writer slot.
            conn.execute_batch("BEGIN IMMEDIATE")
                .expect("begin immediate");
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('busy_probe', '1')",
                [],
            )
            .expect("foreign write");
            thread::sleep(Duration::from_millis(HOLD_MS));
            conn.execute_batch("COMMIT").expect("commit");
        });

        // Let the holder acquire the lock, then contend from the real binary.
        thread::sleep(Duration::from_millis(SETTLE_MS));
        let out = h.run(&["--json", "playlist", "create", "Contended"]);
        holder.join().expect("holder thread");

        assert_eq!(
            code(&out),
            0,
            "the CLI write must wait out the foreign txn, not fail busy"
        );
    });

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
