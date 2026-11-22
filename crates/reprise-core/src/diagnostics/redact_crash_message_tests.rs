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

#[test]
fn a_line_without_the_structure_of_a_symbol_is_no_frame() {
    let backtrace =
        "backtrace:\n   4: 192.168.1.5\n   5: key=abc123\n   6: Time.flac\n   7: password hunter2";
    let (_, shown) = written_and_shown("boom", backtrace);

    for leaked in ["192.168", "abc123", "Time", "hunter2"] {
        assert!(!shown.contains(leaked), "{leaked:?} leaked:\n{shown}");
    }
}

#[test]
fn a_frame_line_is_a_frame_only_inside_the_backtrace() {
    let tail = "---- Reprise crash report ----\nmessage: boom\n   7: core::ops::call_once{{vtable.shim}}\nbacktrace:\n   8: core::ops::call_once{{vtable.shim}}\n---- end of crash report ----";
    let facts = DiagnosticFacts {
        log_tail: Some(tail.to_owned()),
        ..DiagnosticFacts::default()
    };
    let shown = render_report(&facts, &DiagnosticLog::default(), &context());

    assert!(shown.contains("\n   7: …"), "{shown}");
    assert!(
        shown.contains("\n   8: core::ops::call_once{{vtable.shim}}"),
        "{shown}"
    );
}

#[test]
fn the_source_text_of_an_assertion_is_no_cover_for_a_value() {
    for message in [
        "assertion failed: addr == 192.168.1.5",
        "assertion failed: self.token == abc123",
        "assertion failed: self.cover == Time.flac",
        "assertion failed: token == \"abc123\"",
        "assertion failed: path == \"Pink Floyd/the wall\"",
    ] {
        let (written, shown) = written_and_shown(message, "backtrace:");
        for text in [message_line(&written), message_line(&shown)] {
            for leaked in ["192.168", "abc123", "Time", "Pink", "Floyd"] {
                assert!(
                    !text.contains(leaked),
                    "{leaked:?} leaked from {message:?}: {text}"
                );
            }
        }
    }
}

#[test]
fn a_frame_with_a_value_in_a_path_is_no_frame() {
    let backtrace = "backtrace:\n   4: a::b=hunter2\n   5: <x>::192.168.1.5\n   6: core::x::token abc123\n   7: <Pink as Floyd>::Time.flac";
    let (_, shown) = written_and_shown("boom", backtrace);

    for leaked in ["hunter2", "192.168", "abc123", "Time"] {
        assert!(!shown.contains(leaked), "{leaked:?} leaked:\n{shown}");
    }
}

#[test]
fn the_source_text_of_an_assertion_is_no_cover_for_an_assignment() {
    for message in [
        "assertion failed: password=hunter2 && a.b()",
        "assertion failed: token: abc123 == a.b",
        "assertion failed: a.b() && key=abc123",
        "assertion failed: a.b() && secret:hunter2",
    ] {
        let (written, shown) = written_and_shown(message, "backtrace:");
        for text in [message_line(&written), message_line(&shown)] {
            for leaked in ["hunter2", "abc123"] {
                assert!(
                    !text.contains(leaked),
                    "{leaked:?} leaked from {message:?}: {text}"
                );
            }
        }
    }
}

/// An exemption may skip a pass only where the pass would leave the text
/// alone: whatever condition the panic message keeps, the network, secret and
/// assignment passes read it as it is.
#[test]
fn a_condition_that_stays_readable_is_one_no_pass_would_change() {
    use super::redact_assignments::redact_sensitive_assignments;
    use super::redact_network::redact_network_identifiers;
    use super::redact_secrets::secret_cut;

    let atoms = [
        "a",
        "self.b()",
        "x.len()",
        "token",
        "password",
        "key",
        "abc123",
        "192.168.1.5",
        "==",
        "!=",
        "<",
        "&&",
        "=",
        ":",
        "::",
        "a::b",
        "q[0]",
        "hunter2",
        "Time.flac",
        "id=3",
        "k:v",
        "(",
        ")",
        "$HOME",
        "…",
        "token:",
        "pass=",
        "0xC0A80105",
        "::x",
        "-",
        "!",
        "s.t=",
        "u: ",
    ];
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut kept = 0;
    for _ in 0..200_000 {
        let length = 1 + next() % 6;
        let words: Vec<&str> = (0..length)
            .map(|_| atoms[(next() % atoms.len() as u64) as usize])
            .collect();
        let condition = words.join(" ");
        let message = format!("assertion failed: {condition}");
        if super::redact_panic_message(&message, &context()) != message {
            continue;
        }
        kept += 1;
        let tokens: Vec<&str> = condition.split_inclusive(char::is_whitespace).collect();
        let values_only = condition
            .split_whitespace()
            .filter(|token| !token.chars().all(|ch| "=!<>&|+-*%^".contains(ch)))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(
            redact_network_identifiers(&condition),
            condition,
            "{message}"
        );
        assert_eq!(
            redact_sensitive_assignments(&values_only),
            values_only,
            "{message}"
        );
        assert!(
            (0..tokens.len()).all(|from| secret_cut(&tokens[from..]).is_none()),
            "{message}"
        );
    }
    assert!(kept > 100, "the fuzz kept only {kept} conditions");
}
