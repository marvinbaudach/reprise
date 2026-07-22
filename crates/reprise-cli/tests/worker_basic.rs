//! Worker host basics. Only compiled/run with `--features worker`; the default
//! `cargo test -p reprise-cli` skips this file.
#![cfg(feature = "worker")]

mod common;

use std::path::{Path, PathBuf};

use common::{code, parse_json, stderr, Harness};

fn staging_dir(h: &Harness) -> PathBuf {
    h.dir.path().join("staging")
}

/// Configures a library root under the temp dir (promotions land under
/// `<root>/Reprise Instrumentals/…`).
fn set_library_root(h: &Harness) -> PathBuf {
    let root = h.dir.path().join("library");
    std::fs::create_dir_all(&root).unwrap();
    reprise_core::library::settings::set_library_root(&h.conn(), root.to_str().unwrap()).unwrap();
    root
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

    // The render is published to its canonical path via a rename; no
    // claim-scoped `.partial` temp file may linger after a clean run.
    let leftover: Vec<_> = std::fs::read_dir(&staging)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "partial")
        })
        .collect();
    assert!(
        leftover.is_empty(),
        "no .partial temp renders should remain: {leftover:?}"
    );
}

#[test]
fn save_intent_is_auto_promoted_by_the_worker() {
    let h = Harness::new();
    let staging = staging_dir(&h);
    let root = set_library_root(&h);
    h.seed_track_with_file(1);

    // Default (save) create — no --stage — persists the auto-promote intent on
    // the job (decision 15).
    let created = parse_json(&h.run(&[
        "--json",
        "--staging-dir",
        staging.to_str().unwrap(),
        "instrumental",
        "create",
        "1",
    ]));
    assert_eq!(created["save"], "save");
    let job_id = created["jobs"][0]["job_id"].as_i64().unwrap();

    // A single worker pass renders the job and, honoring the persisted intent,
    // promotes it in the same run — no manual `instrumental save` step.
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
    assert_eq!(parse_json(&out)["done"], 1);

    // The job is now saved: a result track is attached and its staging render is
    // gone (promotion moved it into the library).
    let status = parse_json(&h.run(&[
        "--json",
        "--staging-dir",
        staging.to_str().unwrap(),
        "jobs",
        "status",
    ]));
    assert_eq!(status[0]["state"], "done");
    let result_track = status[0]["result_track_id"].as_i64();
    assert!(
        result_track.is_some_and(|id| id >= 1),
        "the render was auto-promoted to a library track without a manual save: {status}"
    );
    let store = reprise_core::ai_staging::StagingStore::new(&staging);
    assert!(!store.exists(job_id), "the promoted render leaves staging");

    // Filed under the dedicated subfolder with the "(Instrumental)" suffix.
    let expected = root
        .join("Reprise Instrumentals")
        .join("Artist 1")
        .join("Song 1 (Instrumental).flac");
    assert!(
        expected.is_file(),
        "the promoted file exists at {}",
        expected.display()
    );
}

#[test]
fn worker_rejects_a_zero_lease() {
    let h = Harness::new();
    // A zero (or negative) lease would make every claim instantly reclaimable
    // and defeat the leasing model — clap must reject it up front.
    let out = h.run(&["jobs", "work", "--once", "--fake-backend", "--lease", "0"]);
    assert_eq!(code(&out), 2, "an out-of-range --lease is a usage error");
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
