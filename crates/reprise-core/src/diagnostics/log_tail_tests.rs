use super::{read_log_tail, tail_of, TAIL_MAX_BYTES, TAIL_MAX_LINES};
use crate::crash_report::{render_report, CRASH_REPORT_MARKER};
use crate::diagnostics::RedactionContext;

#[test]
fn a_short_log_is_kept_whole() {
    assert_eq!(tail_of("one\ntwo\n", false).as_deref(), Some("one\ntwo"));
}

#[test]
fn only_the_last_lines_survive() {
    let text: String = (0..TAIL_MAX_LINES + 5)
        .map(|n| format!("line {n}\n"))
        .collect();

    let tail = tail_of(&text, false).unwrap();

    assert_eq!(tail.lines().count(), TAIL_MAX_LINES);
    assert!(tail.starts_with("line 5\n"));
    assert!(tail.ends_with(&format!("line {}", TAIL_MAX_LINES + 4)));
}

#[test]
fn a_cut_start_drops_the_fragment_of_the_first_line() {
    assert_eq!(tail_of("rtial\nwhole\n", true).as_deref(), Some("whole"));
}

#[test]
fn blank_text_has_no_tail() {
    assert_eq!(tail_of("", false), None);
    assert_eq!(tail_of("\n \n", false), None);
}

#[test]
fn a_missing_file_has_no_tail() {
    let root = tempfile::tempdir().unwrap();

    assert_eq!(read_log_tail(&root.path().join("absent.log")), None);
}

#[test]
fn a_huge_file_is_read_only_from_its_end() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("reprise.log");
    let filler = "x".repeat(100);
    let mut contents: String = (0..2_000).map(|_| format!("{filler}\n")).collect();
    contents.push_str("the last line\n");
    assert!(contents.len() as u64 > TAIL_MAX_BYTES * 4);
    std::fs::write(&path, contents).unwrap();

    let tail = read_log_tail(&path).unwrap();

    assert!(tail.ends_with("the last line"));
    assert!(tail.len() as u64 <= TAIL_MAX_BYTES);
    assert!(tail
        .lines()
        .all(|line| line.len() == 100 || line == "the last line"));
}

/// A backtrace of `frames` frames, two lines each, like a GTK application's.
fn long_backtrace(frames: usize) -> String {
    (0..frames)
        .map(|n| {
            format!(
                "  {n}: reprise_gnome::ui::window::some_module::some_function_name::{{{{closure}}}}\n             at /rustc/0123456789abcdef/library/core/src/ops/function.rs:250:5"
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn log_ending_in_a_crash(frames: usize, lines_after: usize) -> String {
    let mut log: String = (0..200)
        .map(|n| format!("INFO earlier line {n}\n"))
        .collect();
    log.push_str(&render_report(
        "index out of bounds: the len is 3 but the index is 7",
        "crates/reprise-gnome/src/ui/window/window.rs:101:17",
        &long_backtrace(frames),
        &RedactionContext::default(),
        "2026-11-01T10:00:00Z",
    ));
    for n in 0..lines_after {
        log.push_str(&format!("INFO after the crash {n}\n"));
    }
    log
}

#[test]
fn a_crash_report_keeps_its_message_and_location_whatever_the_backtrace_length() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("reprise.previous.log");
    let log = log_ending_in_a_crash(120, 0);
    assert!(
        log.len() as u64 > TAIL_MAX_BYTES,
        "the report alone outgrows the byte window"
    );
    std::fs::write(&path, log).unwrap();

    let tail = read_log_tail(&path).unwrap();

    assert!(tail.starts_with(CRASH_REPORT_MARKER), "{tail}");
    assert!(tail.contains("message: index out of bounds: the len is 3 but the index is 7"));
    assert!(tail.contains("location: crates/reprise-gnome/src/ui/window/window.rs:101:17"));
    assert!(
        tail.contains("backtrace:\n  0: reprise_gnome"),
        "the backtrace is cut from its end"
    );
    assert!(!tail.contains("earlier line"));
    assert!(tail.lines().count() <= TAIL_MAX_LINES);
    assert!(tail.len() as u64 <= TAIL_MAX_BYTES);
}

#[test]
fn the_last_crash_report_wins_and_lines_after_it_are_bounded_too() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("reprise.previous.log");
    let mut log = log_ending_in_a_crash(3, 2);
    log.push_str(&log_ending_in_a_crash(100, 5));
    std::fs::write(&path, log).unwrap();

    let tail = read_log_tail(&path).unwrap();

    assert!(tail.starts_with(CRASH_REPORT_MARKER));
    assert_eq!(tail.matches(CRASH_REPORT_MARKER).count(), 1);
    assert!(tail.contains("message: index out of bounds"));
    assert!(tail.lines().count() <= TAIL_MAX_LINES);
}

#[test]
fn a_huge_panic_message_cannot_push_the_location_out_of_the_bound() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("reprise.previous.log");
    let log = render_report(
        &"x".repeat(100_000),
        "crates/reprise-core/src/lib.rs:1:1",
        &long_backtrace(10),
        &RedactionContext::default(),
        "2026-11-01T10:00:00Z",
    );
    std::fs::write(&path, log).unwrap();

    let tail = read_log_tail(&path).unwrap();

    assert!(tail.len() as u64 <= TAIL_MAX_BYTES);
    assert!(tail.contains("location: crates/reprise-core/src/lib.rs:1:1"));
}
