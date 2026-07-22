mod common;

use common::{code, parse_json, stdout, Harness};

fn staging_arg(h: &Harness) -> String {
    h.dir.path().join("staging").to_string_lossy().into_owned()
}

/// Enqueues a batch through the CLI and returns its batch id.
fn seed_batch(h: &Harness, staging: &str, ids: &[&str]) -> String {
    let mut args = vec!["--json", "--staging-dir", staging, "instrumental", "create"];
    args.extend_from_slice(ids);
    let value = parse_json(&h.run(&args));
    value["batch_id"].as_str().unwrap().to_string()
}

#[test]
fn status_on_an_empty_queue_is_friendly() {
    let h = Harness::new();
    let out = h.run(&["jobs", "status"]);
    assert_eq!(code(&out), 0);
    assert!(stdout(&out).contains("no jobs"));

    let json = parse_json(&h.run(&["--json", "jobs", "status"]));
    assert_eq!(json.as_array().unwrap().len(), 0);
}

#[test]
fn status_lists_a_queued_job_with_zero_progress_and_no_result() {
    let h = Harness::new();
    h.seed_tracks(1);
    let staging = staging_arg(&h);
    h.run(&["--staging-dir", &staging, "instrumental", "create", "1"]);

    let json = parse_json(&h.run(&["--json", "--staging-dir", &staging, "jobs", "status"]));
    let rows = json.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["state"], "queued");
    assert_eq!(rows[0]["progress_permille"], 0);
    assert_eq!(rows[0]["source_track_id"], 1);
    assert!(rows[0]["result_track_id"].is_null());
    assert_eq!(rows[0]["staged"], false, "a queued job has no render yet");
}

#[test]
fn status_batch_reports_aggregate_progress() {
    let h = Harness::new();
    h.seed_tracks(2);
    let staging = staging_arg(&h);
    let batch = seed_batch(&h, &staging, &["1", "2"]);

    let json = parse_json(&h.run(&[
        "--json",
        "--staging-dir",
        &staging,
        "jobs",
        "status",
        "--batch",
        &batch,
    ]));
    assert_eq!(json["batch"]["batch_id"], batch);
    assert_eq!(json["batch"]["total"], 2);
    assert_eq!(json["batch"]["queued"], 2);
    assert_eq!(json["batch"]["done"], 0);
    assert_eq!(json["batch"]["progress_permille"], 0);
    assert_eq!(json["jobs"].as_array().unwrap().len(), 2);
}

#[test]
fn status_shows_a_done_staged_job_as_staged() {
    let h = Harness::new();
    h.seed_tracks(1);
    let staging = staging_arg(&h);
    let created = parse_json(&h.run(&[
        "--json",
        "--staging-dir",
        &staging,
        "instrumental",
        "create",
        "1",
    ]));
    let job_id = created["jobs"][0]["job_id"].as_i64().unwrap();
    // Arrange a finished render on disk for this job.
    h.stage_done_render(std::path::Path::new(&staging), job_id);

    let json = parse_json(&h.run(&["--json", "--staging-dir", &staging, "jobs", "status"]));
    let rows = json.as_array().unwrap();
    assert_eq!(rows[0]["state"], "done");
    assert_eq!(rows[0]["staged"], true);
    assert!(
        rows[0]["result_track_id"].is_null(),
        "done but not yet saved"
    );
}
