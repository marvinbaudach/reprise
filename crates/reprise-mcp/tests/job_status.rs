//! `music_get_job_status`: read-only job metadata by ids and/or batch id.
//! Covers batch listing + aggregate progress, id lookup reflecting progress and
//! the saved result track, the `library:read` gate, the "needs an argument"
//! guard, unknown ids, and the D19 no-leak guarantee (never a path or staging
//! location).

mod common;

use std::path::Path;

use common::{
    seed_real_flac_track, set_bool_setting, structured_ok, tool_error_text, McpClient, SeedTrack,
    CAP_AI_CREATE,
};
use serde_json::{json, Value};
use tempfile::TempDir;

const CAP_LIBRARY_READ: &str = "agent.capability.library:read";

/// Enqueues one `queued` job for `source_id` straight through the core facade
/// (no capability needed to *set up* status fixtures).
fn enqueue_job(db: &Path, source_id: i64) -> i64 {
    let conn = reprise_core::db::open_migrated(Some(db)).unwrap();
    let staging = reprise_core::ai_staging::StagingStore::new(db.parent().unwrap().join("staging"));
    reprise_core::ai_jobs::enqueue_instrumental(&conn, &staging, source_id, "test@1", 1_000)
        .unwrap()
        .job_id()
}

fn jobs_array(structured: &Value) -> &Vec<Value> {
    structured
        .get("jobs")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("expected a jobs array: {structured}"))
}

fn first_job_id(structured: &Value) -> i64 {
    jobs_array(structured)
        .first()
        .and_then(|job| job.get("job_id"))
        .and_then(Value::as_i64)
        .expect("a job_id")
}

#[test]
fn reports_a_batch_with_aggregate_progress() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("reprise.db");
    let ids = common::seed_tracks(
        &db,
        &[
            SeedTrack::simple("One", "Artist"),
            SeedTrack::simple("Two", "Artist"),
        ],
    );
    set_bool_setting(&db, CAP_AI_CREATE, true);
    let mut client = McpClient::start(&db);

    let created =
        structured_ok(&client.call_tool("music_create_instrumental", json!({ "track_ids": ids })));
    let batch_id = created
        .get("batch_id")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();

    let status =
        structured_ok(&client.call_tool("music_get_job_status", json!({ "batch_id": batch_id })));

    let jobs = jobs_array(&status);
    assert_eq!(jobs.len(), 2, "both batch jobs are reported");
    for job in jobs {
        assert_eq!(job.get("state").and_then(Value::as_str), Some("queued"));
        assert_eq!(
            job.get("progress_permille").and_then(Value::as_u64),
            Some(0)
        );
    }
    let batch = status.get("batch").expect("aggregate batch present");
    assert_eq!(batch.get("total").and_then(Value::as_i64), Some(2));
    assert_eq!(batch.get("queued").and_then(Value::as_i64), Some(2));
    assert_eq!(batch.get("permille").and_then(Value::as_u64), Some(0));
}

#[test]
fn by_job_ids_reports_done_and_the_saved_result_track() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("reprise.db");
    let staging = dir.path().join("staging");
    let music = dir.path().join("music");
    let library = dir.path().join("library");
    std::fs::create_dir_all(&music).unwrap();
    let (source_id, _flac) = seed_real_flac_track(&db, &music, "Creep", "Radiohead");
    set_bool_setting(&db, CAP_AI_CREATE, true);
    let mut client = McpClient::start(&db);

    let created = structured_ok(&client.call_tool(
        "music_create_instrumental",
        json!({ "track_ids": [source_id] }),
    ));
    let job_id = first_job_id(&created);

    // Render (staged) then promote.
    common::run_worker_until_idle(&db, &staging);
    let done =
        structured_ok(&client.call_tool("music_get_job_status", json!({ "job_ids": [job_id] })));
    let job = &jobs_array(&done)[0];
    assert_eq!(job.get("state").and_then(Value::as_str), Some("done"));
    assert_eq!(
        job.get("progress_permille").and_then(Value::as_u64),
        Some(1000)
    );
    assert!(
        job.get("result_track_id").is_none_or(Value::is_null),
        "a staged, unsaved render has no result track yet: {job}"
    );

    let result_track_id = common::promote_job(&db, &staging, &library, job_id);
    let saved =
        structured_ok(&client.call_tool("music_get_job_status", json!({ "job_ids": [job_id] })));
    assert_eq!(
        jobs_array(&saved)[0]
            .get("result_track_id")
            .and_then(Value::as_i64),
        Some(result_track_id),
        "after promotion the saved library track id appears"
    );
}

#[test]
fn reflects_a_running_job_progress() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("reprise.db");
    let ids = common::seed_tracks(&db, &[SeedTrack::simple("One", "Artist")]);
    let job_id = enqueue_job(&db, ids[0]);
    // Claim it and post partial progress via the facade — no MCP write needed.
    {
        let conn = reprise_core::db::open_migrated(Some(&db)).unwrap();
        let claimed = reprise_core::ai_jobs::claim_next(&conn, 7, 2_000, 300)
            .unwrap()
            .unwrap();
        assert_eq!(claimed.id, job_id);
        assert!(reprise_core::ai_jobs::set_progress(&conn, job_id, 7, 500).unwrap());
    }
    let mut client = McpClient::start(&db);

    let status =
        structured_ok(&client.call_tool("music_get_job_status", json!({ "job_ids": [job_id] })));
    let job = &jobs_array(&status)[0];
    assert_eq!(job.get("state").and_then(Value::as_str), Some("running"));
    assert_eq!(
        job.get("progress_permille").and_then(Value::as_u64),
        Some(500)
    );
}

#[test]
fn refused_when_library_read_is_revoked() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("reprise.db");
    let ids = common::seed_tracks(&db, &[SeedTrack::simple("One", "Artist")]);
    let job_id = enqueue_job(&db, ids[0]);
    set_bool_setting(&db, CAP_LIBRARY_READ, false);
    let mut client = McpClient::start(&db);

    let response = client.call_tool("music_get_job_status", json!({ "job_ids": [job_id] }));
    let text = tool_error_text(&response);
    assert!(
        text.contains("library:read"),
        "job status is gated on library:read: {text}"
    );
}

#[test]
fn requires_job_ids_or_a_batch_id() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("reprise.db");
    common::seed_tracks(&db, &[SeedTrack::simple("One", "Artist")]);
    let mut client = McpClient::start(&db);

    let response = client.call_tool("music_get_job_status", json!({}));
    let text = tool_error_text(&response);
    assert!(
        text.contains("job_ids") && text.contains("batch_id"),
        "an empty query should ask for an argument: {text}"
    );
}

#[test]
fn an_unknown_job_id_is_silently_absent() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("reprise.db");
    common::seed_tracks(&db, &[SeedTrack::simple("One", "Artist")]);
    let mut client = McpClient::start(&db);

    let status =
        structured_ok(&client.call_tool("music_get_job_status", json!({ "job_ids": [999_999] })));
    assert!(
        jobs_array(&status).is_empty(),
        "an unknown id yields no job rows, not an error: {status}"
    );
}

#[test]
fn status_never_leaks_paths_even_for_a_track_with_a_revealing_path() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("reprise.db");
    // A deliberately revealing source path that must never surface in status.
    let ids = common::seed_tracks(
        &db,
        &[SeedTrack {
            path: "/home/marvin/Music/secret-folder/track.flac".to_string(),
            title: "Track".to_string(),
            artist: "Artist".to_string(),
            album: "Album".to_string(),
            genre: "Jazz".to_string(),
            year: Some(2001),
            duration_ms: 200_000,
            rating: 3,
        }],
    );
    let job_id = enqueue_job(&db, ids[0]);
    let mut client = McpClient::start(&db);

    let response = client.call_tool("music_get_job_status", json!({ "job_ids": [job_id] }));
    common::assert_no_leaks(&serde_json::to_string(&response).unwrap());
}
