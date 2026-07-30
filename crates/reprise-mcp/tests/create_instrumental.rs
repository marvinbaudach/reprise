//! `music_create_instrumental`: the AI-create capability gate (fail-closed off,
//! immediate revocation), the immediate job/batch registration, dedup
//! references (Beschluss 16), the save/stage split (Beschluss 15), the honest
//! queued hint, and the input limits. The happy path drives a real worker
//! in-process against the same temp DB and promotes the render, proving an
//! MCP-registered job is promotable end-to-end.

mod common;

use common::{
    run_worker_completing, run_worker_until_idle, seed_real_flac_track, set_bool_setting,
    structured_ok, tool_error_text, McpClient, SeedTrack, CAP_AI_CREATE,
};
use serde_json::{json, Value};
use tempfile::TempDir;

/// A DB seeded with `count` fake-path tracks (fine for tests that never render).
fn db_with_tracks(dir: &TempDir, count: usize) -> (std::path::PathBuf, Vec<i64>) {
    let path = dir.path().join("reprise.db");
    let tracks: Vec<SeedTrack> = (0..count)
        .map(|i| SeedTrack::simple(&format!("Track{i}"), "Artist"))
        .collect();
    let ids = common::seed_tracks(&path, &tracks);
    (path, ids)
}

fn first_job_id(structured: &Value) -> i64 {
    structured
        .get("jobs")
        .and_then(Value::as_array)
        .and_then(|jobs| jobs.first())
        .and_then(|job| job.get("job_id"))
        .and_then(Value::as_i64)
        .unwrap_or_else(|| panic!("expected a job_id: {structured}"))
}

#[test]
fn refused_when_capability_off_by_default() {
    let dir = TempDir::new().unwrap();
    let (path, ids) = db_with_tracks(&dir, 1);
    // No capability set — fail-closed off (Beschluss 7).
    let mut client = McpClient::start(&path);

    let response = client.call_tool("music_create_instrumental", json!({ "track_ids": ids }));
    let text = tool_error_text(&response);

    assert!(
        text.contains("ai:create"),
        "refusal should name the capability: {text}"
    );
    assert_eq!(
        common::count_ai_jobs(&path),
        0,
        "a refused call must not enqueue any job"
    );
}

#[test]
fn revocation_takes_effect_immediately_mid_session() {
    let dir = TempDir::new().unwrap();
    let (path, ids) = db_with_tracks(&dir, 2);
    set_bool_setting(&path, CAP_AI_CREATE, true);
    let mut client = McpClient::start(&path);

    // Granted at startup: the first call registers a job.
    let granted = client.call_tool(
        "music_create_instrumental",
        json!({ "track_ids": [ids[0]] }),
    );
    structured_ok(&granted);

    // Revoke while the server keeps running — re-read on every call refuses the
    // next one immediately.
    set_bool_setting(&path, CAP_AI_CREATE, false);
    let revoked = client.call_tool(
        "music_create_instrumental",
        json!({ "track_ids": [ids[1]] }),
    );
    let text = tool_error_text(&revoked);
    assert!(
        text.contains("ai:create"),
        "revocation should refuse: {text}"
    );
    assert_eq!(
        common::count_ai_jobs(&path),
        1,
        "only the first (granted) call enqueued a job"
    );
}

#[test]
fn save_true_registers_a_job_that_promotes_end_to_end() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("reprise.db");
    let staging = dir.path().join("staging");
    let music = dir.path().join("music");
    let library = dir.path().join("library");
    std::fs::create_dir_all(&music).unwrap();
    let (source_id, _flac) = seed_real_flac_track(&db, &music, "Creep", "Radiohead");
    set_bool_setting(&db, CAP_AI_CREATE, true);
    let mut client = McpClient::start(&db);

    let response = client.call_tool(
        "music_create_instrumental",
        json!({ "track_ids": [source_id], "save": true }),
    );
    let structured = structured_ok(&response);

    // Registered immediately: one new job, no dedup, save echoed, batch id set.
    assert_eq!(structured.get("created").and_then(Value::as_u64), Some(1));
    assert_eq!(
        structured.get("deduplicated").and_then(Value::as_u64),
        Some(0)
    );
    assert_eq!(structured.get("save").and_then(Value::as_bool), Some(true));
    assert!(
        structured
            .get("batch_id")
            .and_then(Value::as_str)
            .is_some_and(|id| !id.is_empty()),
        "a batch id groups the invocation: {structured}"
    );
    assert!(
        structured
            .get("queued_hint")
            .and_then(Value::as_str)
            .is_some_and(|hint| !hint.is_empty()),
        "the queued hint is present: {structured}"
    );
    let job_id = first_job_id(&structured);

    // Drive the in-process worker the way the real CLI worker runs: it renders
    // and, honoring the save=true intent, promotes the render in the same
    // completion — no manual save step, against the same temp DB the server used.
    run_worker_completing(&db, &staging, &library);

    // The MCP-registered job carried all the way to a saved library track.
    let (state, saved_track) = common::job_state(&db, job_id);
    assert_eq!(state, "done");
    let result_track_id = saved_track.expect("save=true auto-promotes to a library track");
    assert_ne!(result_track_id, source_id, "a new track was created");

    let handle = reprise_core::db::Db::open_migrated(Some(&db)).unwrap();
    let provenance = reprise_core::provenance::get_provenance(&handle, result_track_id)
        .unwrap()
        .expect("the promoted track has a provenance row");
    assert!(provenance.ai, "the promoted track is flagged AI");
    assert_eq!(provenance.source_track_id, Some(source_id));
}

#[test]
fn save_false_stages_the_render_in_the_conversion_view() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("reprise.db");
    let staging = dir.path().join("staging");
    let music = dir.path().join("music");
    std::fs::create_dir_all(&music).unwrap();
    let (source_id, _flac) = seed_real_flac_track(&db, &music, "Karma Police", "Radiohead");
    set_bool_setting(&db, CAP_AI_CREATE, true);
    let mut client = McpClient::start(&db);

    let response = client.call_tool(
        "music_create_instrumental",
        json!({ "track_ids": [source_id], "save": false }),
    );
    let structured = structured_ok(&response);
    assert_eq!(structured.get("save").and_then(Value::as_bool), Some(false));
    let job_id = first_job_id(&structured);

    // The worker renders it, but with save=false it stays staged (done, no
    // result track) awaiting the user's decision.
    run_worker_until_idle(&db, &staging);
    let (state, saved_track) = common::job_state(&db, job_id);
    assert_eq!(state, "done");
    assert_eq!(saved_track, None, "save=false leaves the render unsaved");

    // save=false routed through the conversion playlist, so the staging view's
    // sidebar entry exists, and the render is on disk to play.
    let handle = reprise_core::db::Db::open_migrated(Some(&db)).unwrap();
    assert!(
        reprise_core::ai_conversion::conversion_playlist(&handle)
            .unwrap()
            .is_some(),
        "save=false ensures the Conversion playlist exists"
    );
    let render = reprise_core::ai_staging::StagingStore::new(&staging);
    assert!(render.exists(job_id), "the staged render is on disk");
}

#[test]
fn a_repeated_track_is_deduplicated_with_a_reference() {
    let dir = TempDir::new().unwrap();
    let (path, ids) = db_with_tracks(&dir, 1);
    set_bool_setting(&path, CAP_AI_CREATE, true);
    let mut client = McpClient::start(&path);

    let first = structured_ok(&client.call_tool(
        "music_create_instrumental",
        json!({ "track_ids": [ids[0]] }),
    ));
    let first_job = first_job_id(&first);

    // Re-triggering the same track references the existing open job — no second
    // render (Beschluss 16).
    let second = structured_ok(&client.call_tool(
        "music_create_instrumental",
        json!({ "track_ids": [ids[0]] }),
    ));
    assert_eq!(second.get("created").and_then(Value::as_u64), Some(0));
    assert_eq!(second.get("deduplicated").and_then(Value::as_u64), Some(1));
    let job = second
        .get("jobs")
        .and_then(Value::as_array)
        .and_then(|jobs| jobs.first())
        .unwrap();
    assert_eq!(job.get("deduplicated").and_then(Value::as_bool), Some(true));
    assert_eq!(job.get("job_id").and_then(Value::as_i64), Some(first_job));
    assert_eq!(common::count_ai_jobs(&path), 1, "no second job row exists");
}

#[test]
fn rejects_an_empty_track_list() {
    let dir = TempDir::new().unwrap();
    let (path, _ids) = db_with_tracks(&dir, 1);
    set_bool_setting(&path, CAP_AI_CREATE, true);
    let mut client = McpClient::start(&path);

    let response = client.call_tool("music_create_instrumental", json!({ "track_ids": [] }));
    let text = tool_error_text(&response);
    assert!(
        text.contains("at least one"),
        "empty list should be rejected: {text}"
    );
    assert_eq!(common::count_ai_jobs(&path), 0);
}

#[test]
fn rejects_more_than_500_ids() {
    let dir = TempDir::new().unwrap();
    let (path, _ids) = db_with_tracks(&dir, 1);
    set_bool_setting(&path, CAP_AI_CREATE, true);
    let mut client = McpClient::start(&path);

    let too_many: Vec<i64> = (1..=501).collect();
    let response = client.call_tool(
        "music_create_instrumental",
        json!({ "track_ids": too_many }),
    );
    let text = tool_error_text(&response);
    assert!(text.contains("too many"), "should reject > 500 ids: {text}");
    assert_eq!(common::count_ai_jobs(&path), 0);
}

#[test]
fn rejects_a_track_id_that_is_not_present() {
    let dir = TempDir::new().unwrap();
    let (path, _ids) = db_with_tracks(&dir, 1);
    set_bool_setting(&path, CAP_AI_CREATE, true);
    let mut client = McpClient::start(&path);

    let response = client.call_tool(
        "music_create_instrumental",
        json!({ "track_ids": [999_999] }),
    );
    let text = tool_error_text(&response);
    assert!(
        text.contains("not present"),
        "should reject an absent id: {text}"
    );
    assert!(
        text.contains("999999"),
        "should name the offending id: {text}"
    );
    assert_eq!(common::count_ai_jobs(&path), 0);
}

#[test]
fn queued_hint_is_honest_about_the_worker() {
    let dir = TempDir::new().unwrap();
    let (path, ids) = db_with_tracks(&dir, 1);
    set_bool_setting(&path, CAP_AI_CREATE, true);
    let mut client = McpClient::start(&path);

    let structured = structured_ok(&client.call_tool(
        "music_create_instrumental",
        json!({ "track_ids": [ids[0]] }),
    ));
    let hint = structured
        .get("queued_hint")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(
        hint.contains("queued") && hint.contains("reprise-cli jobs work"),
        "the hint must state jobs stay queued until a worker runs: {hint}"
    );
}
