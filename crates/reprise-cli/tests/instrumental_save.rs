mod common;

use std::path::{Path, PathBuf};

use common::{code, parse_json, stderr, Harness};

fn staging_dir(h: &Harness) -> PathBuf {
    h.dir.path().join("staging")
}

/// Configures a library root under the temp dir (promotion files under
/// `<root>/Reprise Instrumentals/…`).
fn set_library_root(h: &Harness) -> PathBuf {
    let root = h.dir.path().join("library");
    std::fs::create_dir_all(&root).unwrap();
    reprise_core::library::settings::set_library_root(&h.conn(), root.to_str().unwrap()).unwrap();
    root
}

/// Seeds track 1 (with a real FLAC), enqueues its instrumental job, then marks
/// it done with a staged render on disk. Returns the job id.
fn staged_job(h: &Harness, staging: &Path) -> i64 {
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
fn save_promotes_a_staged_render_into_the_library() {
    let h = Harness::new();
    let staging = staging_dir(&h);
    let root = set_library_root(&h);
    let job_id = staged_job(&h, &staging);

    let out = h.run(&[
        "--json",
        "--staging-dir",
        staging.to_str().unwrap(),
        "instrumental",
        "save",
        &job_id.to_string(),
    ]);
    assert_eq!(code(&out), 0);
    let rows = parse_json(&out);
    assert_eq!(rows[0]["status"], "saved");
    let result_track = rows[0]["result_track_id"].as_i64().unwrap();
    assert!(result_track >= 1);

    // Filed under the dedicated subfolder with the "(Instrumental)" suffix.
    let expected = root
        .join("Reprise Instrumentals")
        .join("Artist 1")
        .join("Song 1 (Instrumental).flac");
    assert_eq!(
        rows[0]["path"].as_str().unwrap(),
        expected.to_string_lossy()
    );
    assert!(expected.is_file(), "the promoted file exists");

    // The job is now saved (result attached) and its staging render is gone.
    let store = reprise_core::ai_staging::StagingStore::new(&staging);
    assert!(!store.exists(job_id), "staging render discarded after save");
    let status = parse_json(&h.run(&[
        "--json",
        "--staging-dir",
        staging.to_str().unwrap(),
        "jobs",
        "status",
    ]));
    assert_eq!(status[0]["result_track_id"].as_i64().unwrap(), result_track);
}

#[test]
fn save_without_a_library_root_is_invalid_input() {
    let h = Harness::new();
    let staging = staging_dir(&h);
    let job_id = staged_job(&h, &staging); // note: no library root configured

    let out = h.run(&[
        "--staging-dir",
        staging.to_str().unwrap(),
        "instrumental",
        "save",
        &job_id.to_string(),
    ]);
    assert_eq!(code(&out), 7);
    assert!(stderr(&out).contains("library root"));
}

#[test]
fn save_a_queued_job_is_rejected_as_not_promotable() {
    let h = Harness::new();
    let staging = staging_dir(&h);
    set_library_root(&h);
    h.seed_track_with_file(1);
    // Enqueue but do NOT mark done — it is still queued, not a finished render.
    let created = parse_json(&h.run(&[
        "--json",
        "--staging-dir",
        staging.to_str().unwrap(),
        "instrumental",
        "create",
        "1",
    ]));
    let job_id = created["jobs"][0]["job_id"].as_i64().unwrap();

    let out = h.run(&[
        "--staging-dir",
        staging.to_str().unwrap(),
        "instrumental",
        "save",
        &job_id.to_string(),
    ]);
    assert_eq!(code(&out), 7);
    assert!(stderr(&out).contains("not a finished, unsaved render"));
}

#[test]
fn save_a_missing_job_is_not_found() {
    let h = Harness::new();
    let staging = staging_dir(&h);
    set_library_root(&h);
    let out = h.run(&[
        "--staging-dir",
        staging.to_str().unwrap(),
        "instrumental",
        "save",
        "4242",
    ]);
    assert_eq!(code(&out), 3);
}

#[test]
fn saved_instrumental_dedups_a_re_create_with_the_result_track() {
    let h = Harness::new();
    let staging = staging_dir(&h);
    set_library_root(&h);
    let job_id = staged_job(&h, &staging);
    let saved = parse_json(&h.run(&[
        "--json",
        "--staging-dir",
        staging.to_str().unwrap(),
        "instrumental",
        "save",
        &job_id.to_string(),
    ]));
    let result_track = saved[0]["result_track_id"].as_i64().unwrap();

    // Re-creating for the same source now dedups to the saved result.
    let again = parse_json(&h.run(&[
        "--json",
        "--staging-dir",
        staging.to_str().unwrap(),
        "instrumental",
        "create",
        "1",
    ]));
    assert_eq!(again["jobs"][0]["outcome"], "deduplicated");
    assert_eq!(
        again["jobs"][0]["result_track_id"].as_i64().unwrap(),
        result_track
    );
}

#[test]
fn discard_deletes_the_staged_render() {
    let h = Harness::new();
    let staging = staging_dir(&h);
    let job_id = staged_job(&h, &staging);
    let store = reprise_core::ai_staging::StagingStore::new(&staging);
    assert!(store.exists(job_id));

    let out = h.run(&[
        "--json",
        "--staging-dir",
        staging.to_str().unwrap(),
        "instrumental",
        "discard",
        &job_id.to_string(),
    ]);
    assert_eq!(code(&out), 0);
    assert_eq!(parse_json(&out)[0]["status"], "discarded");
    assert!(!store.exists(job_id), "the render file is gone");
    // Discarded jobs leave the conversion view (list_active_jobs excludes them).
    let status = parse_json(&h.run(&[
        "--json",
        "--staging-dir",
        staging.to_str().unwrap(),
        "jobs",
        "status",
    ]));
    assert_eq!(status.as_array().unwrap().len(), 0);
}

#[test]
fn discard_a_job_with_no_staged_render_is_not_found() {
    let h = Harness::new();
    let staging = staging_dir(&h);
    let out = h.run(&[
        "--staging-dir",
        staging.to_str().unwrap(),
        "instrumental",
        "discard",
        "77",
    ]);
    assert_eq!(code(&out), 3);
}
