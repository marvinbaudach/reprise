mod common;

use common::{code, parse_json, stderr, stdout, Harness};

#[test]
fn create_prints_id_and_writes_exactly_one_change_log_row() {
    let h = Harness::new();
    let out = h.run(&["--json", "playlist", "create", "Road Trip"]);
    assert_eq!(code(&out), 0);
    let value = parse_json(&out);
    assert_eq!(value["name"], "Road Trip");
    assert!(value["id"].as_i64().unwrap() >= 1);
    // The single most load-bearing assertion of the whole package: one create
    // == exactly one outbox row (so a running app refreshes once, not zero or
    // twice).
    assert_eq!(h.change_log_len(), 1, "create must log exactly one change");
}

#[test]
fn create_event_targets_the_playlist_entity() {
    let h = Harness::new();
    let created = parse_json(&h.run(&["--json", "playlist", "create", "Focus"]));
    let id = created["id"].as_i64().unwrap();
    let events = parse_json(&h.run(&["--json", "events", "tail"]));
    let rows = events.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["entity"], "playlist");
    assert_eq!(rows[0]["entity_id"], id.to_string());
    assert_eq!(rows[0]["op"], "create");
}

#[test]
fn create_with_tracks_seeds_members_and_logs_once() {
    let h = Harness::new();
    h.seed_tracks(3);
    let out = h.run(&["--json", "playlist", "create", "Mix", "--tracks", "1,2,3"]);
    assert_eq!(code(&out), 0);
    let value = parse_json(&out);
    assert_eq!(value["track_count"], 3);
    assert_eq!(
        h.change_log_len(),
        1,
        "create-with-tracks logs exactly one change"
    );

    let id = value["id"].as_i64().unwrap();
    let shown = parse_json(&h.run(&["--json", "playlist", "show", &id.to_string()]));
    assert_eq!(shown["tracks"].as_array().unwrap().len(), 3);
}

#[test]
fn list_shows_created_playlists() {
    let h = Harness::new();
    h.run(&["playlist", "create", "One"]);
    h.run(&["playlist", "create", "Two"]);
    let out = h.run(&["--json", "playlist", "list"]);
    assert_eq!(code(&out), 0);
    let rows = parse_json(&out);
    let names: Vec<&str> = rows
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["One", "Two"]);
}

#[test]
fn list_text_on_empty_is_friendly() {
    let h = Harness::new();
    let out = h.run(&["playlist", "list"]);
    assert_eq!(code(&out), 0);
    assert!(stdout(&out).contains("no playlists"));
}

#[test]
fn show_reports_header_and_tracks() {
    let h = Harness::new();
    h.seed_tracks(2);
    let id = parse_json(&h.run(&["--json", "playlist", "create", "P", "--tracks", "1,2"]))["id"]
        .as_i64()
        .unwrap();
    let out = h.run(&["--json", "playlist", "show", &id.to_string()]);
    assert_eq!(code(&out), 0);
    let value = parse_json(&out);
    assert_eq!(value["name"], "P");
    assert_eq!(value["track_count"], 2);
    assert_eq!(value["tracks"][0]["title"], "Song 1");
}

#[test]
fn show_missing_playlist_is_not_found_exit_3() {
    let h = Harness::new();
    let out = h.run(&["playlist", "show", "999"]);
    assert_eq!(code(&out), 3);
    assert!(stderr(&out).contains("playlist 999 not found"));
}

#[test]
fn rename_changes_the_name() {
    let h = Harness::new();
    let id = parse_json(&h.run(&["--json", "playlist", "create", "Old"]))["id"]
        .as_i64()
        .unwrap();
    let out = h.run(&["playlist", "rename", &id.to_string(), "New"]);
    assert_eq!(code(&out), 0);
    let rows = parse_json(&h.run(&["--json", "playlist", "list"]));
    assert_eq!(rows[0]["name"], "New");
}

#[test]
fn rename_missing_playlist_is_not_found_and_logs_nothing() {
    let h = Harness::new();
    let out = h.run(&["playlist", "rename", "42", "Whatever"]);
    assert_eq!(code(&out), 3);
    // Pre-checking existence means we never log a phantom rename.
    assert_eq!(h.change_log_len(), 0);
}

#[test]
fn delete_requires_yes_and_is_a_no_op_without_it() {
    let h = Harness::new();
    let id = parse_json(&h.run(&["--json", "playlist", "create", "Doomed"]))["id"]
        .as_i64()
        .unwrap();
    let out = h.run(&["playlist", "delete", &id.to_string()]);
    assert_eq!(
        code(&out),
        4,
        "missing --yes is a confirmation-required exit"
    );
    assert!(stderr(&out).contains("--yes"));
    // Still present: create logged one event, the refused delete logged none.
    assert_eq!(h.change_log_len(), 1);
    let rows = parse_json(&h.run(&["--json", "playlist", "list"]));
    assert_eq!(rows.as_array().unwrap().len(), 1);
}

#[test]
fn delete_with_yes_removes_the_playlist_and_logs_it() {
    let h = Harness::new();
    let id = parse_json(&h.run(&["--json", "playlist", "create", "Doomed"]))["id"]
        .as_i64()
        .unwrap();
    let out = h.run(&["--json", "playlist", "delete", &id.to_string(), "--yes"]);
    assert_eq!(code(&out), 0);
    assert_eq!(parse_json(&out)["deleted"], true);
    let rows = parse_json(&h.run(&["--json", "playlist", "list"]));
    assert!(rows.as_array().unwrap().is_empty());
    // create + delete == two change-log rows.
    assert_eq!(h.change_log_len(), 2);
}

#[test]
fn delete_missing_playlist_is_not_found_even_with_yes() {
    let h = Harness::new();
    let out = h.run(&["playlist", "delete", "7", "--yes"]);
    assert_eq!(code(&out), 3);
}
