use super::{read_log_tail, tail_of, TAIL_MAX_BYTES, TAIL_MAX_LINES};
use super::log_tail::crash_section_of;
use crate::crash_report::{render_report, CRASH_REPORT_END_MARKER, CRASH_REPORT_MARKER};
use crate::diagnostics::PanicPayload;

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
    let path = root.path().join("run.log");
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
        &PanicPayload::Formatted("index out of bounds: the len is 3 but the index is 7".to_owned()),
        Some(("crates/reprise-gnome/src/ui/window/window.rs", 101, 17)),
        &long_backtrace(frames),
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
    assert!(tail.contains("message: index out of bounds: the len is … but the index is …"));
    assert!(tail.contains("location: crates/reprise-gnome/src/ui/window/window.rs:101:17"));
    assert!(
        tail.contains("backtrace:\n   0: reprise_gnome"),
        "the backtrace is cut from its end"
    );
    assert!(!tail.contains("earlier line"));
    assert!(tail.lines().count() <= TAIL_MAX_LINES);
    assert!(tail.len() as u64 <= TAIL_MAX_BYTES);
}

#[test]
fn the_last_crash_report_wins() {
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

// The panic hook writes the report before it logs, so the hook's own ERROR line
// and other threads' lines follow the end marker: they are not part of it.
#[test]
fn a_crash_section_stops_at_the_end_of_the_report() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("reprise.previous.log");
    let mut log = log_ending_in_a_crash(3, 0);
    log.push_str("ERROR panic panic_payload=zqxuser\n");
    log.push_str("INFO after the crash 0\n");
    std::fs::write(&path, log).unwrap();

    let tail = read_log_tail(&path).unwrap();

    assert!(tail.starts_with(CRASH_REPORT_MARKER), "{tail}");
    assert!(tail.ends_with(CRASH_REPORT_END_MARKER), "{tail}");
    assert!(!tail.contains("zqxuser"), "{tail}");
    assert!(!tail.contains("after the crash"), "{tail}");
}

#[test]
fn a_report_cut_short_runs_to_the_end_of_the_log() {
    let log = format!("{CRASH_REPORT_MARKER}\ntime: t\nmessage: m\nINFO later\n");

    let section = crash_section_of(&log).unwrap();

    assert!(section.ends_with("INFO later"), "{section}");
}

#[test]
fn a_huge_panic_message_cannot_push_the_location_out_of_the_bound() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("reprise.previous.log");
    let huge: &'static str = Box::leak("x".repeat(100_000).into_boxed_str());
    let log = render_report(
        &PanicPayload::Static(huge),
        Some(("crates/reprise-core/src/lib.rs", 1, 1)),
        &long_backtrace(10),
        "2026-11-01T10:00:00Z",
    );
    std::fs::write(&path, log).unwrap();

    let tail = read_log_tail(&path).unwrap();

    assert!(tail.len() as u64 <= TAIL_MAX_BYTES);
    assert!(tail.contains("location: crates/reprise-core/src/lib.rs:1:1"));
}
