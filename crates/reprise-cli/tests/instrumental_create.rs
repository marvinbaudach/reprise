mod common;

use common::{code, parse_json, stderr, stdout, Harness};

/// A `--staging-dir` inside the harness temp dir so nothing touches the real
/// per-user staging store.
fn staging_arg(h: &Harness) -> String {
    h.dir.path().join("staging").to_string_lossy().into_owned()
}

#[test]
fn create_enqueues_a_job_and_logs_one_event() {
    let h = Harness::new();
    h.seed_tracks(1);
    let out = h.run(&[
        "--json",
        "--staging-dir",
        &staging_arg(&h),
        "instrumental",
        "create",
        "1",
    ]);
    assert_eq!(code(&out), 0);
    let value = parse_json(&out);
    assert_eq!(value["save"], "save", "save is the automation default");
    assert!(value["batch_id"].is_null(), "a single id is not a batch");
    let jobs = value["jobs"].as_array().unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0]["outcome"], "created");
    assert_eq!(jobs[0]["source_track_id"], 1);
    assert!(jobs[0]["job_id"].as_i64().unwrap() >= 1);
    // Exactly one lifecycle event (the enqueue) so a running app refreshes once.
    assert_eq!(h.change_log_len(), 1);
    assert_eq!(h.ai_job_row_count(), 1);
}

#[test]
fn create_multiple_ids_forms_one_batch() {
    let h = Harness::new();
    h.seed_tracks(3);
    let out = h.run(&[
        "--json",
        "--staging-dir",
        &staging_arg(&h),
        "instrumental",
        "create",
        "1",
        "2",
        "3",
    ]);
    assert_eq!(code(&out), 0);
    let value = parse_json(&out);
    assert!(
        value["batch_id"].is_string(),
        "several ids share a batch id"
    );
    let jobs = value["jobs"].as_array().unwrap();
    assert_eq!(jobs.len(), 3);
    assert!(jobs.iter().all(|j| j["outcome"] == "created"));
    assert_eq!(h.ai_job_row_count(), 3);
    // One enqueue event per job.
    assert_eq!(h.change_log_len(), 3);
}

#[test]
fn create_is_stage_mode_under_stage_flag() {
    let h = Harness::new();
    h.seed_tracks(1);
    let out = h.run(&[
        "--json",
        "--staging-dir",
        &staging_arg(&h),
        "instrumental",
        "create",
        "1",
        "--stage",
    ]);
    assert_eq!(code(&out), 0);
    assert_eq!(parse_json(&out)["save"], "stage");
}

#[test]
fn create_dedups_the_second_request_with_a_reference_not_a_second_render() {
    let h = Harness::new();
    h.seed_tracks(1);
    let staging = staging_arg(&h);
    let first = parse_json(&h.run(&[
        "--json",
        "--staging-dir",
        &staging,
        "instrumental",
        "create",
        "1",
    ]));
    let first_job = first["jobs"][0]["job_id"].as_i64().unwrap();

    let second = parse_json(&h.run(&[
        "--json",
        "--staging-dir",
        &staging,
        "instrumental",
        "create",
        "1",
    ]));
    // Beschluss 16: a skip that references the existing job, exit 0.
    assert_eq!(second["jobs"][0]["outcome"], "deduplicated");
    assert_eq!(second["jobs"][0]["job_id"].as_i64().unwrap(), first_job);
    assert!(second["jobs"][0]["result_track_id"].is_null());
    // Still exactly one job and one enqueue event — no double render.
    assert_eq!(h.ai_job_row_count(), 1);
    assert_eq!(h.change_log_len(), 1);
}

#[test]
fn create_conflicting_save_and_stage_is_a_usage_error() {
    let h = Harness::new();
    h.seed_tracks(1);
    let out = h.run(&[
        "--staging-dir",
        &staging_arg(&h),
        "instrumental",
        "create",
        "1",
        "--save",
        "--stage",
    ]);
    assert_eq!(code(&out), 2, "clap rejects mutually exclusive flags");
}

#[test]
fn create_missing_track_is_not_found_and_enqueues_nothing() {
    let h = Harness::new();
    let out = h.run(&[
        "--staging-dir",
        &staging_arg(&h),
        "instrumental",
        "create",
        "404",
    ]);
    assert_eq!(code(&out), 3);
    assert!(stderr(&out).contains("track 404 not found"));
    assert_eq!(h.ai_job_row_count(), 0);
}

#[test]
fn create_batch_is_all_or_nothing_on_a_missing_id() {
    let h = Harness::new();
    h.seed_tracks(1);
    let out = h.run(&[
        "--staging-dir",
        &staging_arg(&h),
        "instrumental",
        "create",
        "1",
        "999",
    ]);
    assert_eq!(code(&out), 3);
    // The valid id must not have been enqueued when a sibling is invalid.
    assert_eq!(h.ai_job_row_count(), 0);
}

#[test]
fn create_text_output_is_honest_about_needing_a_worker() {
    let h = Harness::new();
    h.seed_tracks(1);
    let out = h.run(&[
        "--staging-dir",
        &staging_arg(&h),
        "instrumental",
        "create",
        "1",
    ]);
    assert_eq!(code(&out), 0);
    let text = stdout(&out);
    assert!(text.contains("queued job"));
    assert!(
        text.contains("jobs work") || text.contains("Reprise app"),
        "output must name how the jobs get processed: {text:?}"
    );
}
