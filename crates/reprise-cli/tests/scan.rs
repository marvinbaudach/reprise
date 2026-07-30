mod common;

use common::{code, parse_json, Harness};

/// A folder holding one unreadable audio file, so a scan reliably *touches*
/// the library (records an import error) and therefore logs a change event —
/// without needing a real decodable track fixture.
fn music_folder_with_broken_track() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().expect("music dir");
    std::fs::write(dir.path().join("broken.mp3"), b"this is not audio").expect("write file");
    dir
}

#[test]
fn scan_completes_and_logs_a_single_scan_event() {
    let h = Harness::new();
    let music = music_folder_with_broken_track();
    let out = h.run(&["--json", "scan", music.path().to_str().unwrap()]);
    assert_eq!(code(&out), 0);
    assert_eq!(parse_json(&out)["outcome"], "completed");

    let events = parse_json(&h.run(&["--json", "events", "tail"]));
    let rows = events.as_array().unwrap();
    assert_eq!(
        rows.len(),
        1,
        "a touching scan logs exactly one collective event"
    );
    assert_eq!(rows[0]["entity"], "library");
    assert_eq!(rows[0]["op"], "scan");
}

#[test]
fn scan_missing_root_is_unavailable_exit_8() {
    let h = Harness::new();
    let out = h.run(&["--json", "scan", "/definitely/not/here/reprise-test"]);
    assert_eq!(code(&out), 8);
    assert_eq!(parse_json(&out)["outcome"], "root_unavailable");
}

#[test]
fn scan_without_path_or_configured_root_is_invalid_input_exit_7() {
    let h = Harness::new();
    let out = h.run(&["scan"]);
    assert_eq!(code(&out), 7);
}

#[test]
fn scan_without_path_uses_the_configured_library_root() {
    let h = Harness::new();
    let music = music_folder_with_broken_track();
    let db = h.db();
    reprise_core::library::settings::set_library_root(&db, music.path().to_str().unwrap()).unwrap();
    let out = h.run(&["--json", "scan"]);
    assert_eq!(code(&out), 0);
    assert_eq!(parse_json(&out)["outcome"], "completed");
}
