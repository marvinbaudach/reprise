mod common;

use std::path::{Path, PathBuf};

use common::{code, parse_json, Harness};

fn staging_dir(h: &Harness) -> PathBuf {
    h.dir.path().join("staging")
}

fn set_library_root(h: &Harness) -> PathBuf {
    let root = h.dir.path().join("library");
    std::fs::create_dir_all(&root).unwrap();
    reprise_core::library::settings::set_library_root(&h.conn(), root.to_str().unwrap()).unwrap();
    root
}

/// Enqueues track 1's job (no wait) and drops a finished render in place, so a
/// subsequent `create --wait` dedups onto a job that is already `done` — the
/// deterministic way to exercise the wait path without a live worker.
fn prestaged_job(h: &Harness, staging: &Path) -> i64 {
    h.seed_track_with_file(1);
    let created = parse_json(&h.run(&[
        "--json",
        "--staging-dir",
        staging.to_str().unwrap(),
        "instrumental",
        "create",
        "1",
        "--stage",
    ]));
    let job_id = created["jobs"][0]["job_id"].as_i64().unwrap();
    h.stage_done_render(staging, job_id);
    job_id
}

#[test]
fn wait_times_out_honestly_when_no_worker_runs() {
    let h = Harness::new();
    h.seed_tracks(1);
    let staging = staging_dir(&h);
    // No worker is running, so the queued job never progresses: --wait must
    // give up and say so (plan 3.2), exiting non-zero.
    let out = h.run(&[
        "--json",
        "--staging-dir",
        staging.to_str().unwrap(),
        "instrumental",
        "create",
        "1",
        "--stage",
        "--wait",
        "--wait-timeout",
        "0",
    ]);
    assert_eq!(code(&out), 8, "a timeout is an Unavailable exit");
    let value = parse_json(&out);
    assert_eq!(value["waited"], true);
    assert_eq!(value["jobs"][0]["status"], "timeout");
    assert_eq!(value["jobs"][0]["state"], "queued");
}

#[test]
fn wait_save_promotes_a_finished_render() {
    let h = Harness::new();
    let staging = staging_dir(&h);
    set_library_root(&h);
    let job_id = prestaged_job(&h, &staging);

    // `create --wait` (default save) dedups onto the finished job and, seeing it
    // done, promotes it — one command drives create -> observe -> save.
    let out = h.run(&[
        "--json",
        "--staging-dir",
        staging.to_str().unwrap(),
        "instrumental",
        "create",
        "1",
        "--wait",
        "--wait-timeout",
        "5",
    ]);
    assert_eq!(code(&out), 0);
    let value = parse_json(&out);
    assert_eq!(value["save"], "save");
    assert_eq!(value["jobs"][0]["status"], "saved");
    let result_track = value["jobs"][0]["result_track_id"].as_i64().unwrap();
    assert!(result_track >= 1);
    assert!(value["jobs"][0]["path"]
        .as_str()
        .unwrap()
        .contains("(Instrumental)"));

    // The job now carries its saved result and its staging render is gone.
    assert_eq!(h.ai_job_status(job_id).as_deref(), Some("done"));
    let store = reprise_core::ai_staging::StagingStore::new(&staging);
    assert!(!store.exists(job_id));
}

#[test]
fn wait_stage_leaves_the_render_in_staging() {
    let h = Harness::new();
    let staging = staging_dir(&h);
    // No library root: stage mode never promotes, so none is needed.
    let job_id = prestaged_job(&h, &staging);

    let out = h.run(&[
        "--json",
        "--staging-dir",
        staging.to_str().unwrap(),
        "instrumental",
        "create",
        "1",
        "--stage",
        "--wait",
        "--wait-timeout",
        "5",
    ]);
    assert_eq!(code(&out), 0);
    assert_eq!(parse_json(&out)["jobs"][0]["status"], "staged");

    // Still staged, still unsaved.
    let store = reprise_core::ai_staging::StagingStore::new(&staging);
    assert!(store.exists(job_id), "stage mode keeps the render");
}

#[test]
fn wait_save_without_a_library_root_fails_fast() {
    let h = Harness::new();
    h.seed_tracks(1);
    let staging = staging_dir(&h);
    // save + wait needs somewhere to promote to; refuse before waiting.
    let out = h.run(&[
        "--staging-dir",
        staging.to_str().unwrap(),
        "instrumental",
        "create",
        "1",
        "--wait",
        "--wait-timeout",
        "5",
    ]);
    assert_eq!(code(&out), 7, "no library root is an invalid-input exit");
    assert_eq!(
        h.ai_job_row_count(),
        0,
        "it fails before enqueuing anything"
    );
}
