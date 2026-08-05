//! Library Doctor MCP tools over the real stdio JSON-RPC boundary.

mod common;

use std::path::{Path, PathBuf};

use common::{assert_no_leaks, set_bool_setting, structured_ok, tool_error_text, McpClient};
use reprise_core::library::tag_edit::{apply_patch_to_file, TagPatch};
use serde_json::json;
use tempfile::TempDir;

const CAP_TAGS_WRITE: &str = "agent.capability.tags:write";
const DOCTOR_SCAN_REQUEST: &str = include_str!("fixtures/doctor_scan_request.json");
const DOCTOR_SCAN_RESPONSE: &str = include_str!("fixtures/doctor_scan_response.json");

fn fixture_track(dir: &Path, name: &str, title: &str) -> PathBuf {
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../reprise-core/tests/fixtures/sine.flac");
    let music = dir.join("music");
    std::fs::create_dir_all(&music).unwrap();
    let path = music.join(name);
    std::fs::copy(source, &path).unwrap();
    apply_patch_to_file(
        &path,
        &TagPatch {
            title: Some(title.into()),
            artist: Some("Test Artist".into()),
            album: Some("Test Album".into()),
            album_artist: Some("Test Artist".into()),
            genre: Some("Rock".into()),
            ..TagPatch::default()
        },
    )
    .unwrap();
    path
}

fn fixture_db(dir: &TempDir, tracks: &[PathBuf]) -> (PathBuf, Vec<i64>) {
    let path = dir.path().join("reprise.db");
    let db = reprise_core::db::Db::open_migrated(Some(&path)).unwrap();
    let root = tracks[0].parent().unwrap();
    reprise_core::library::scanner::scan_folder(&db, root).unwrap();
    drop(db);
    let conn = common::fixture_connection(&path);
    let ids = tracks
        .iter()
        .map(|track| {
            conn.query_row(
                "SELECT id FROM tracks WHERE path=?1",
                [track.to_string_lossy().as_ref()],
                |row| row.get(0),
            )
            .unwrap()
        })
        .collect();
    (path, ids)
}

fn scan_selection(client: &mut McpClient, track_ids: &[i64]) -> i64 {
    let response = client.call_tool(
        "music_scan_tags",
        json!({
            "scope": "selection",
            "track_ids": track_ids,
            "remote": false,
            "apply_safe": false
        }),
    );
    structured_ok(&response)["scan_id"].as_i64().unwrap()
}

fn insert_review_proposal(
    db_path: &Path,
    scan_id: i64,
    position: i64,
    track_id: i64,
    problem_class: &str,
) {
    common::fixture_connection(db_path)
        .execute(
            "INSERT INTO library_doctor_proposals \
             (scan_id, position, track_id, field, current_value, proposed_value, source, \
              confidence, preselected, problem_class, evidence_json, local_fallback_json) \
             VALUES (?1, ?2, ?3, 'album_artist', 'Test Artist', 'Canonical Artist', \
                     'musicbrainz', 90, 0, ?4, '[]', 'null')",
            rusqlite::params![scan_id, position, track_id, problem_class],
        )
        .unwrap();
}

fn insert_conflict_group(db_path: &Path, scan_id: i64, track_id: i64) {
    let conn = common::fixture_connection(db_path);
    conn.execute(
        "INSERT INTO library_doctor_groups \
         (scan_id, position, field, group_key, local_fallback_json) \
         VALUES (?1, 0, 'album_artist', 'album-artist:conflict', 'null')",
        [scan_id],
    )
    .unwrap();
    let group_id = conn.last_insert_rowid();
    for (position, candidate) in ["Canonical Artist", "Other Artist"].iter().enumerate() {
        conn.execute(
            "INSERT INTO library_doctor_group_candidates \
             (group_id, position, candidate_value, candidate_count, evidence_json) \
             VALUES (?1, ?2, ?3, 1, '[]')",
            rusqlite::params![group_id, position as i64, candidate],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO library_doctor_group_members \
         (group_id, position, track_id, current_value) VALUES (?1, 0, ?2, 'Test Artist')",
        rusqlite::params![group_id, track_id],
    )
    .unwrap();
}

#[test]
fn doc_11a_scan_tags_does_not_write_without_apply_safe() {
    let dir = TempDir::new().unwrap();
    let track = fixture_track(dir.path(), "spaced.flac", "  Spaced title  ");
    let (path, ids) = fixture_db(&dir, &[track]);
    let mut client = McpClient::start(&path);

    let response = client.call_tool(
        "music_scan_tags",
        json!({
            "scope": "selection",
            "track_ids": ids,
            "remote": false,
            "apply_safe": false
        }),
    );
    let result = structured_ok(&response);

    assert_eq!(result["applied"], 0);
    assert_eq!(result["checked"], 1);
    assert_eq!(
        common::fixture_connection(&path)
            .query_row("SELECT COUNT(*) FROM tag_write_jobs", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn doc_11a_review_tags_groups_by_album_and_filters_by_category() {
    let dir = TempDir::new().unwrap();
    let tracks = vec![
        fixture_track(dir.path(), "one.flac", "One"),
        fixture_track(dir.path(), "two.flac", "Two"),
    ];
    let (path, ids) = fixture_db(&dir, &tracks);
    let mut client = McpClient::start(&path);
    let scan_id = scan_selection(&mut client, &ids);
    for (position, track_id) in ids.iter().copied().enumerate() {
        insert_review_proposal(
            &path,
            scan_id,
            position as i64,
            track_id,
            "casing_whitespace",
        );
    }

    let casing = structured_ok(&client.call_tool(
        "music_review_tags",
        json!({ "category": "casing", "limit": 20, "offset": 0 }),
    ));
    let year = structured_ok(&client.call_tool(
        "music_review_tags",
        json!({ "category": "year", "limit": 20, "offset": 0 }),
    ));

    assert_eq!(casing["albums"].as_array().unwrap().len(), 1);
    assert_eq!(casing["albums"][0]["change_count"], 2);
    assert_eq!(year["albums"].as_array().unwrap().len(), 0);
}

#[test]
fn doc_11a_apply_safe_requires_the_tags_write_capability() {
    let dir = TempDir::new().unwrap();
    let track = fixture_track(dir.path(), "apply-safe.flac", "  Apply safe  ");
    let (path, ids) = fixture_db(&dir, &[track]);
    let mut client = McpClient::start(&path);

    let denied_by_default = client.call_tool(
        "music_scan_tags",
        json!({
            "scope": "selection",
            "track_ids": ids,
            "remote": false,
            "apply_safe": true
        }),
    );
    assert!(tool_error_text(&denied_by_default).contains("tags:write"));

    set_bool_setting(&path, CAP_TAGS_WRITE, true);
    let denied_until_restart = client.call_tool(
        "music_scan_tags",
        json!({
            "scope": "selection",
            "track_ids": ids,
            "remote": false,
            "apply_safe": true
        }),
    );
    assert!(tool_error_text(&denied_until_restart).contains("tags:write"));
    drop(client);

    let mut granted = McpClient::start(&path);
    let applied = structured_ok(&granted.call_tool(
        "music_scan_tags",
        json!({
            "scope": "selection",
            "track_ids": ids,
            "remote": false,
            "apply_safe": true
        }),
    ));
    assert_eq!(applied["applied"], 1);

    set_bool_setting(&path, CAP_TAGS_WRITE, false);
    let revoked = granted.call_tool(
        "music_scan_tags",
        json!({
            "scope": "selection",
            "track_ids": ids,
            "remote": false,
            "apply_safe": true
        }),
    );
    assert!(tool_error_text(&revoked).contains("tags:write"));
}

#[test]
fn doc_11a_apply_tags_requires_the_tags_write_capability() {
    let dir = TempDir::new().unwrap();
    let track = fixture_track(dir.path(), "apply.flac", "Apply");
    let (path, _) = fixture_db(&dir, &[track]);
    let mut client = McpClient::start(&path);

    let response = client.call_tool(
        "music_apply_tags",
        json!({ "action": "apply", "row_ids": [0] }),
    );

    assert!(tool_error_text(&response).contains("tags:write"));
}

#[test]
fn doc_11a_review_tags_counts_written_changes_per_album() {
    let dir = TempDir::new().unwrap();
    let tracks = (1..=11)
        .map(|number| {
            fixture_track(
                dir.path(),
                &format!("album-{number}.flac"),
                &format!("Track {number}"),
            )
        })
        .collect::<Vec<_>>();
    let (path, ids) = fixture_db(&dir, &tracks);
    let mut client = McpClient::start(&path);
    let scan_id = scan_selection(&mut client, &ids);
    for (position, track_id) in ids.iter().copied().enumerate() {
        insert_review_proposal(
            &path,
            scan_id,
            position as i64,
            track_id,
            "casing_whitespace",
        );
    }

    let review = structured_ok(&client.call_tool("music_review_tags", json!({})));

    assert_eq!(review["change_count"], 11);
    assert_eq!(review["albums"][0]["change_count"], 11);
    assert_eq!(review["albums"][0]["rows"].as_array().unwrap().len(), 1);
    assert_eq!(review["albums"][0]["rows"][0]["applies_to_tracks"], 11);
}

#[test]
fn doc_10b_mcp_refuses_while_a_gui_job_holds_the_lock() {
    let dir = TempDir::new().unwrap();
    let track = fixture_track(dir.path(), "busy.flac", "Busy");
    let (path, ids) = fixture_db(&dir, &[track]);
    set_bool_setting(&path, CAP_TAGS_WRITE, true);
    let mut client = McpClient::start(&path);
    let scan_id = scan_selection(&mut client, &ids);
    insert_review_proposal(&path, scan_id, 0, ids[0], "casing_whitespace");
    let review = structured_ok(&client.call_tool("music_review_tags", json!({})));
    let row_id = review["albums"][0]["rows"][0]["row_ids"][0]
        .as_u64()
        .unwrap();
    common::fixture_connection(&path)
        .execute(
            "INSERT INTO tag_write_jobs \
             (kind, source_job_id, scan_id, state, created_at, finished_at, total_tracks) \
             VALUES ('tag_editor', NULL, NULL, 'prepared', 1, NULL, 0)",
            [],
        )
        .unwrap();

    let response = client.call_tool(
        "music_apply_tags",
        json!({ "action": "apply", "row_ids": [row_id] }),
    );

    assert!(tool_error_text(&response).contains("another tag-writing job"));
}

#[test]
fn doc_11a_doctor_responses_carry_no_file_paths() {
    let dir = TempDir::new().unwrap();
    let track = fixture_track(dir.path(), "private.flac", "Private");
    let (path, ids) = fixture_db(&dir, std::slice::from_ref(&track));
    set_bool_setting(&path, CAP_TAGS_WRITE, true);
    let mut client = McpClient::start(&path);

    let scan = client.call_tool(
        "music_scan_tags",
        json!({
            "scope": "selection",
            "track_ids": ids,
            "remote": false,
            "apply_safe": false
        }),
    );
    let scan_id = structured_ok(&scan)["scan_id"].as_i64().unwrap();
    insert_review_proposal(&path, scan_id, 0, ids[0], "casing_whitespace");
    let review = client.call_tool("music_review_tags", json!({}));
    let row_id = structured_ok(&review)["albums"][0]["rows"][0]["row_ids"][0]
        .as_u64()
        .unwrap();
    let apply = client.call_tool(
        "music_apply_tags",
        json!({ "action": "apply", "row_ids": [row_id] }),
    );
    assert_eq!(structured_ok(&apply)["applied"], 1);
    assert_eq!(
        reprise_core::library::tag_edit::read_editable_tags(&track)
            .unwrap()
            .album_artist,
        "Canonical Artist"
    );
    let revert = client.call_tool("music_apply_tags", json!({ "action": "revert" }));
    assert_eq!(structured_ok(&revert)["reverted"], 1);

    for response in [&scan, &review, &apply, &revert] {
        assert_no_leaks(&serde_json::to_string(response).unwrap());
    }
}

#[test]
fn doc_11a_apply_tags_resolves_a_spelling_group() {
    let dir = TempDir::new().unwrap();
    let track = fixture_track(dir.path(), "resolve.flac", "Resolve");
    let (path, ids) = fixture_db(&dir, std::slice::from_ref(&track));
    set_bool_setting(&path, CAP_TAGS_WRITE, true);
    let mut client = McpClient::start(&path);
    let scan_id = scan_selection(&mut client, &ids);
    insert_conflict_group(&path, scan_id, ids[0]);
    let review = structured_ok(&client.call_tool("music_review_tags", json!({})));
    assert_eq!(review["conflicts"][0]["group_key"], "album-artist:conflict");

    let response = client.call_tool(
        "music_apply_tags",
        json!({
            "action": "resolve",
            "group_key": "album-artist:conflict",
            "candidate": "Canonical Artist"
        }),
    );

    assert_eq!(structured_ok(&response)["applied"], 1);
    assert_eq!(
        reprise_core::library::tag_edit::read_editable_tags(&track)
            .unwrap()
            .album_artist,
        "Canonical Artist"
    );
}

#[test]
fn doctor_scan_request_matches_committed_fixture() {
    let dir = TempDir::new().unwrap();
    let track = fixture_track(dir.path(), "fixture.flac", "Fixture");
    let (path, _) = fixture_db(&dir, &[track]);
    let request: serde_json::Value = serde_json::from_str(DOCTOR_SCAN_REQUEST).unwrap();
    let expected: serde_json::Value = serde_json::from_str(DOCTOR_SCAN_RESPONSE).unwrap();
    let arguments = request["params"]["arguments"].clone();
    let mut client = McpClient::start(&path);

    let response = client.call_tool("music_scan_tags", arguments);

    assert_eq!(structured_ok(&response), expected);
}

#[test]
fn doctor_tool_schemas_are_stable() {
    let dir = TempDir::new().unwrap();
    let track = fixture_track(dir.path(), "schema.flac", "Schema");
    let (path, _) = fixture_db(&dir, &[track]);
    let mut client = McpClient::start(&path);
    let response = client.request("tools/list", json!({}));
    let tools = response["result"]["tools"].as_array().unwrap();

    let required = |name: &str| {
        let tool = tools
            .iter()
            .find(|tool| tool["name"] == name)
            .unwrap_or_else(|| panic!("missing {name}: {response}"));
        let mut fields = tool["inputSchema"]["required"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|field| field.as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        fields.sort();
        fields
    };

    assert_eq!(required("music_scan_tags"), ["scope"]);
    assert_eq!(required("music_review_tags"), Vec::<String>::new());
    assert_eq!(required("music_apply_tags"), ["action"]);
}
