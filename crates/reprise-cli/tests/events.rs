mod common;

use common::{code, parse_json, stdout, Harness};

#[test]
fn tail_on_fresh_db_is_empty() {
    let h = Harness::new();
    let out = h.run(&["--json", "events", "tail"]);
    assert_eq!(code(&out), 0);
    assert!(parse_json(&out).as_array().unwrap().is_empty());
}

#[test]
fn tail_reports_mutations_in_order() {
    let h = Harness::new();
    h.run(&["playlist", "create", "A"]);
    h.run(&["playlist", "create", "B"]);
    let rows = parse_json(&h.run(&["--json", "events", "tail"]));
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows[0]["id"].as_i64().unwrap() < rows[1]["id"].as_i64().unwrap());
    assert_eq!(rows[0]["op"], "create");
    assert_eq!(rows[1]["op"], "create");
}

#[test]
fn tail_since_filters_older_rows() {
    let h = Harness::new();
    h.run(&["playlist", "create", "A"]);
    let first = parse_json(&h.run(&["--json", "events", "tail"]));
    let first_id = first[0]["id"].as_i64().unwrap();
    h.run(&["playlist", "create", "B"]);

    let newer = parse_json(&h.run(&["--json", "events", "tail", "--since", &first_id.to_string()]));
    let newer = newer.as_array().unwrap();
    assert_eq!(newer.len(), 1, "only the row after --since is returned");
    assert!(newer[0]["id"].as_i64().unwrap() > first_id);
}

#[test]
fn tail_json_includes_the_writer_token() {
    let h = Harness::new();
    h.run(&["playlist", "create", "A"]);
    let rows = parse_json(&h.run(&["--json", "events", "tail"]));
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert!(
        rows[0]["writer"].is_i64(),
        "writer token should be an integer: {}",
        rows[0]
    );
}

#[test]
fn tail_text_on_empty_is_friendly() {
    let h = Harness::new();
    let out = h.run(&["events", "tail"]);
    assert_eq!(code(&out), 0);
    assert!(stdout(&out).contains("no changes"));
}
