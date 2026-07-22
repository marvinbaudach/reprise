//! Worker host basics. Only compiled/run with `--features worker`; the default
//! `cargo test -p reprise-cli` skips this file.
#![cfg(feature = "worker")]

mod common;

use std::path::{Path, PathBuf};

use common::{code, parse_json, stderr, Harness};

fn staging_dir(h: &Harness) -> PathBuf {
    h.dir.path().join("staging")
}

/// Enqueues instrumental jobs (staged) for `ids` through the CLI, each backed by
/// a real source file so the worker's fake backend can copy it through.
fn enqueue_jobs(h: &Harness, staging: &Path, ids: &[i64]) -> Vec<i64> {
    for &id in ids {
        h.seed_track_with_file(id);
    }
    let mut args = vec![
        "--json",
        "--staging-dir",
        staging.to_str().unwrap(),
        "instrumental",
        "create",
    ];
    let id_strings: Vec<String> = ids.iter().map(i64::to_string).collect();
    for id in &id_strings {
        args.push(id);
    }
    args.push("--stage");
    let created = parse_json(&h.run(&args));
    created["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|j| j["job_id"].as_i64().unwrap())
        .collect()
}

#[test]
fn worker_without_fake_backend_reports_the_backend_is_not_wired() {
    let h = Harness::new();
    let staging = staging_dir(&h);
    enqueue_jobs(&h, &staging, &[1]);
    // No real backend exists yet (reprise-stems is a stub): the worker must say
    // so and change nothing, rather than silently faking a render.
    let out = h.run(&[
        "--staging-dir",
        staging.to_str().unwrap(),
        "jobs",
        "work",
        "--once",
    ]);
    assert_eq!(code(&out), 8, "no backend wired is an Unavailable exit");
    assert!(stderr(&out).contains("reprise-stems"));
    assert_eq!(
        h.ai_job_status(1).as_deref(),
        Some("queued"),
        "the job is untouched"
    );
}

#[test]
fn worker_drains_the_queue_and_renders_every_job() {
    let h = Harness::new();
    let staging = staging_dir(&h);
    let job_ids = enqueue_jobs(&h, &staging, &[1, 2, 3]);

    let out = h.run(&[
        "--json",
        "--staging-dir",
        staging.to_str().unwrap(),
        "jobs",
        "work",
        "--once",
        "--fake-backend",
    ]);
    assert_eq!(code(&out), 0);
    let report = parse_json(&out);
    assert_eq!(report["processed"], 3);
    assert_eq!(report["done"], 3);

    // Every job is done, staged (unsaved), at 100% — and its render is on disk.
    let store = reprise_core::ai_staging::StagingStore::new(&staging);
    let status = parse_json(&h.run(&[
        "--json",
        "--staging-dir",
        staging.to_str().unwrap(),
        "jobs",
        "status",
    ]));
    for row in status.as_array().unwrap() {
        assert_eq!(row["state"], "done");
        assert_eq!(row["progress_permille"], 1000);
        assert!(
            row["result_track_id"].is_null(),
            "worker renders, it does not save"
        );
    }
    for id in job_ids {
        assert!(store.exists(id), "job {id} has a staging render");
    }
}

#[test]
fn worker_on_an_empty_queue_exits_immediately() {
    let h = Harness::new();
    let staging = staging_dir(&h);
    let out = h.run(&[
        "--json",
        "--staging-dir",
        staging.to_str().unwrap(),
        "jobs",
        "work",
        "--once",
        "--fake-backend",
    ]);
    assert_eq!(code(&out), 0);
    assert_eq!(parse_json(&out)["processed"], 0);
}

#[test]
fn worker_honors_max_jobs() {
    let h = Harness::new();
    let staging = staging_dir(&h);
    enqueue_jobs(&h, &staging, &[1, 2, 3]);
    let out = h.run(&[
        "--json",
        "--staging-dir",
        staging.to_str().unwrap(),
        "jobs",
        "work",
        "--once",
        "--fake-backend",
        "--max-jobs",
        "2",
    ]);
    assert_eq!(code(&out), 0);
    assert_eq!(parse_json(&out)["processed"], 2, "stops after two jobs");
    // Exactly one job is left queued.
    let status = parse_json(&h.run(&[
        "--json",
        "--staging-dir",
        staging.to_str().unwrap(),
        "jobs",
        "status",
    ]));
    let queued = status
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["state"] == "queued")
        .count();
    assert_eq!(queued, 1);
}
