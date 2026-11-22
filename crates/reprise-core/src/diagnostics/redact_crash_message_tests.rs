//! The panic message of a crash report keeps the standard phrases and the Rust
//! syntax of the program's own code readable, in the file as written and in the
//! report as read, while anything a user's data can reach is still redacted.

use super::log_tail::crash_section_of;
use super::{render_report, DiagnosticFacts, DiagnosticLog, RedactionContext};

fn context() -> RedactionContext {
    RedactionContext {
        music_dir: Some("/home/marvin/Music".into()),
        home_dir: Some("/home/marvin".into()),
        username: Some("marvin".into()),
        hostname: Some("my-nas".into()),
    }
}

/// The crash report the panic hook writes, the section the report reads back
/// from the log file, and that section as the report shows it.
fn written_and_shown(message: &str, backtrace: &str) -> (String, String) {
    let written = crate::crash_report::render_report(
        message,
        "crates/reprise-core/src/x.rs:1:1",
        backtrace,
        &context(),
        "2026-11-22T10:00:00+00:00",
    );
    let log = format!("2026-11-08T12:00:00.123Z  INFO reprise: starting\n{written}");
    let section = crash_section_of(&log).unwrap_or_default();
    let facts = DiagnosticFacts {
        log_tail: Some(section),
        ..DiagnosticFacts::default()
    };
    let shown = render_report(&facts, &DiagnosticLog::default(), &context());
    (written, shown)
}

fn message_line(text: &str) -> String {
    text.lines()
        .find(|line| line.starts_with("message: "))
        .unwrap_or_default()
        .to_owned()
}

/// The message comes out of both the file and the report as `expected`.
fn assert_message_kept(message: &str, expected: &str) {
    let (written, shown) = written_and_shown(message, "backtrace:");
    assert_eq!(message_line(&written), format!("message: {expected}"));
    assert_eq!(message_line(&shown), format!("message: {expected}"));
}

#[test]
fn the_standard_panic_phrases_stay_readable() {
    for (message, expected) in [
        (
            "called `Result::unwrap()` on an `Err` value: …",
            "called `Result::unwrap()` on an `Err` value: …",
        ),
        (
            "called `Option::unwrap()` on a `None` value",
            "called `Option::unwrap()` on a `None` value",
        ),
        (
            "assertion `left == right` failed",
            "assertion `left == right` failed",
        ),
        (
            "assertion `left != right` failed: queue",
            "assertion `left != right` failed: queue",
        ),
        (
            "internal error: entered unreachable code: state=Playing",
            "internal error: entered unreachable code: state=Playing",
        ),
        ("not yet implemented", "not yet implemented"),
        (
            "assertion failed: index < self.len()",
            "assertion failed: index < self.len()",
        ),
        (
            "assertion failed: !self.queue.is_empty()",
            "assertion failed: !self.queue.is_empty()",
        ),
        (
            "index out of bounds: the len is 3 but the index is 7",
            "index out of bounds: the len is 3 but the index is 7",
        ),
    ] {
        assert_message_kept(message, expected);
    }
}

#[test]
fn what_follows_a_standard_phrase_is_still_redacted() {
    for message in [
        "called `Result::unwrap()` on an `Err` value: Os { path: \"/home/marvin/Music/Pink Floyd/x.flac\" }",
        "called `Result::unwrap()` on an `Err` value: could not open /mnt/usb/Pink Floyd/x.flac",
        "assertion `left == right` failed: could not open /mnt/usb/Pink Floyd",
        "internal error: entered unreachable code: password hunter2",
        "internal error: entered unreachable code: /home/marvin/Music/Pink Floyd",
        "assertion failed: Pink Floyd",
        "assertion failed: marvin",
        "called `Result::unwrap()` on an `Err` value: marvin at my-nas",
    ] {
        let (written, shown) = written_and_shown(message, "backtrace:");
        for text in [message_line(&written), message_line(&shown)] {
            for leaked in ["Pink", "Floyd", "hunter2", "marvin", "my-nas"] {
                assert!(!text.contains(leaked), "{leaked:?} leaked from {message:?}: {text}");
            }
        }
        assert_eq!(
            message_line(&written),
            message_line(&shown),
            "the report changed what the file holds: {message:?}"
        );
    }
}

#[test]
fn a_message_that_only_resembles_a_standard_phrase_is_redacted_as_text() {
    for message in [
        "oops: called `Result::unwrap()` on an `Err` value: password hunter2",
        "expected assertion failed: password hunter2",
    ] {
        let (written, shown) = written_and_shown(message, "backtrace:");
        assert!(!written.contains("hunter2"), "{written}");
        assert!(!shown.contains("hunter2"), "{shown}");
    }
}

#[test]
fn a_backtrace_symbol_stays_readable_in_the_report() {
    let backtrace = "backtrace:\n   0: reprise::ui::run\n   7: <F as core::ops::function::FnOnce<A>>::call_once{{vtable.shim}}\n   8: core::ops::function::FnOnce::call_once{{vtable.shim}}\n   9: std::sys::backtrace::__rust_begin_short_backtrace::<fn(), ()>\n  10: __libc_start_call_main\n  11: main";
    let (_, shown) = written_and_shown("boom", backtrace);

    for line in backtrace.lines().skip(1) {
        assert!(shown.contains(line), "{line:?} lost:\n{shown}");
    }
}

#[test]
fn a_line_that_only_looks_like_a_frame_is_redacted() {
    let backtrace =
        "backtrace:\n   5: Pink Floyd/the wall\n   6: password hunter2\n   7: marvin at my-nas";
    let (written, shown) = written_and_shown("boom", backtrace);

    for leaked in ["Pink", "Floyd", "hunter2", "marvin", "my-nas"] {
        assert!(!shown.contains(leaked), "{leaked:?} leaked:\n{shown}");
    }
    assert!(written.contains("boom"));
}
