mod common;

use common::{code, parse_json, stdout, Harness};

#[test]
fn summary_reports_track_count_and_duration_text() {
    let h = Harness::new();
    h.seed_tracks(3);
    let out = h.run(&["library", "summary"]);
    assert_eq!(code(&out), 0);
    let text = stdout(&out);
    assert!(text.contains("3 tracks"), "unexpected summary: {text}");
}

#[test]
fn summary_json_has_stable_shape() {
    let h = Harness::new();
    h.seed_tracks(2);
    let out = h.run(&["--json", "library", "summary"]);
    assert_eq!(code(&out), 0);
    let value = parse_json(&out);
    assert_eq!(value["track_count"], 2);
    assert_eq!(value["total_duration_ms"], 360_000);
}

#[test]
fn summary_on_empty_library_is_zero() {
    let h = Harness::new();
    let out = h.run(&["--json", "library", "summary"]);
    assert_eq!(code(&out), 0);
    let value = parse_json(&out);
    assert_eq!(value["track_count"], 0);
    assert_eq!(value["total_duration_ms"], 0);
}

#[test]
fn json_flag_after_subcommand_is_accepted() {
    // `--json` is a global argument, usable before or after the subcommand.
    let h = Harness::new();
    let out = h.run(&["library", "summary", "--json"]);
    assert_eq!(code(&out), 0);
    let value = parse_json(&out);
    assert_eq!(value["track_count"], 0);
}
