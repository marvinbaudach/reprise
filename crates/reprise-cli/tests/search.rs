mod common;

use common::{code, parse_json, stdout, Harness};

#[test]
fn search_matches_across_metadata() {
    let h = Harness::new();
    h.seed_tracks(3);
    let out = h.run(&["--json", "search", "Artist"]);
    assert_eq!(code(&out), 0);
    let value = parse_json(&out);
    assert_eq!(value["total"], 3);
    assert_eq!(value["tracks"].as_array().unwrap().len(), 3);
}

#[test]
fn search_json_shape_is_stable() {
    let h = Harness::new();
    h.seed_tracks(1);
    let value = parse_json(&h.run(&["--json", "search", "Song 1"]));
    assert_eq!(value["query"], "Song 1");
    assert_eq!(value["total"], 1);
    assert_eq!(value["offset"], 0);
    assert_eq!(value["limit"], 50);
    let track = &value["tracks"][0];
    assert_eq!(track["title"], "Song 1");
    assert_eq!(track["artist"], "Artist 1");
    assert_eq!(track["duration_ms"], 180_000);
    assert_eq!(track["missing"], false);
}

#[test]
fn search_paginates_with_limit_and_offset() {
    let h = Harness::new();
    h.seed_tracks(5);
    let first = parse_json(&h.run(&[
        "--json", "search", "Artist", "--limit", "2", "--offset", "0",
    ]));
    assert_eq!(first["total"], 5);
    assert_eq!(first["tracks"].as_array().unwrap().len(), 2);

    let second = parse_json(&h.run(&[
        "--json", "search", "Artist", "--limit", "2", "--offset", "2",
    ]));
    assert_eq!(second["tracks"].as_array().unwrap().len(), 2);
    // Windows are disjoint (sorted by title, so ids/titles differ).
    assert_ne!(first["tracks"][0]["id"], second["tracks"][0]["id"]);
}

#[test]
fn search_without_matches_is_empty_but_succeeds() {
    let h = Harness::new();
    h.seed_tracks(2);
    let out = h.run(&["--json", "search", "nonexistent"]);
    assert_eq!(code(&out), 0);
    let value = parse_json(&out);
    assert_eq!(value["total"], 0);
    assert!(value["tracks"].as_array().unwrap().is_empty());
}

#[test]
fn search_text_output_lists_matches() {
    let h = Harness::new();
    h.seed_tracks(2);
    let out = h.run(&["search", "Song 2"]);
    assert_eq!(code(&out), 0);
    assert!(stdout(&out).contains("Song 2"));
}

#[test]
fn search_rejects_negative_limit() {
    let h = Harness::new();
    let out = h.run(&["search", "x", "--limit=-1"]);
    assert_eq!(code(&out), 7, "a negative limit is invalid input");
}
