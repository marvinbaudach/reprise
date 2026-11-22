//! A crash report in the log tail stays readable and still redacts what a
//! user's data can reach: the message text, never the source locations.

use super::{render_report, DiagnosticFacts, DiagnosticLog, RedactionContext};

fn context() -> RedactionContext {
    RedactionContext {
        music_dir: Some("/home/marvin/Music".into()),
        home_dir: Some("/home/marvin".into()),
        username: Some("marvin".into()),
        hostname: Some("my-nas".into()),
    }
}

fn report_with_tail(tail: &str) -> String {
    let facts = DiagnosticFacts {
        log_tail: Some(tail.to_owned()),
        ..DiagnosticFacts::default()
    };
    render_report(&facts, &DiagnosticLog::default(), &context())
}

const CRASH_LINES: [&str; 15] = [
    "---- Reprise crash report ----",
    "time: 2026-11-21T22:00:00Z",
    "version: 0.1.350",
    "schema: 31",
    "location: crates/reprise-gnome/src/ui/window/window.rs:120:9",
    "message: index out of bounds: the len is 3 but the index is 7",
    "backtrace:",
    "   3: reprise::ui::window::build",
    "             at ./crates/reprise-gnome/src/ui/window/window.rs:120:9",
    "   4: core::panicking::panic_bounds_check",
    "             at /rustc/abcdef0123/library/core/src/panicking.rs:280:5",
    "   5: <gtk4::auto::application::Application as glib::object::Cast>::run",
    "             at src/main.rs:12:5",
    "   6: core::f64::consts::PI",
    "---- end of crash report ----",
];

fn crash() -> String {
    CRASH_LINES.join("\n")
}

#[test]
fn a_crash_section_keeps_its_locations_and_its_message() {
    let report = report_with_tail(&crash());

    for readable in [
        "location: crates/reprise-gnome/src/ui/window/window.rs:120:9",
        "message: index out of bounds: the len is 3 but the index is 7",
        "   3: reprise::ui::window::build",
        "             at ./crates/reprise-gnome/src/ui/window/window.rs:120:9",
        "   4: core::panicking::panic_bounds_check",
        "   5: <gtk4::auto::application::Application as glib::object::Cast>::run",
        "             at src/main.rs:12:5",
        "time: 2026-11-21T22:00:00Z",
        "version: 0.1.350",
        "schema: 31",
        "   6: core::f64::consts::PI",
        "---- end of crash report ----",
    ] {
        assert!(report.contains(readable), "{readable:?} lost:\n{report}");
    }
}

#[test]
fn a_panic_message_with_a_user_path_loses_the_path_and_keeps_the_sentence() {
    let tail = crash().replace(
        "index out of bounds: the len is 3 but the index is 7",
        "could not open /home/marvin/Music/Pink Floyd/The Wall/01.flac: not found",
    );

    let report = report_with_tail(&tail);

    assert!(report.contains("message: could not open "), "{report}");
    for leaked in ["Pink", "Floyd", "Wall", "01.flac", "marvin"] {
        assert!(!report.contains(leaked), "{leaked:?} leaked:\n{report}");
    }
}

#[test]
fn a_location_that_is_not_a_source_location_is_redacted() {
    let tail = crash().replace(
        "location: crates/reprise-gnome/src/ui/window/window.rs:120:9",
        "location: /home/marvin/Music/Pink Floyd/x.rs:1:1",
    ) + "\n             at /home/marvin/Music/Pink Floyd/y.rs:2:2";

    let report = report_with_tail(&tail);

    for leaked in ["Pink", "Floyd", "marvin"] {
        assert!(!report.contains(leaked), "{leaked:?} leaked:\n{report}");
    }
}

#[test]
fn a_forged_crash_marker_does_not_make_free_text_readable() {
    let tail = "2026-11-08T12:00:00.123Z WARN x: forged\n\
---- Reprise crash report ----\n\
location: crates/Pink Floyd/the wall.rs:1:1\n\
at crates/pink-floyd/zqxa zqxb.rs:3:4";

    let report = report_with_tail(tail);

    for leaked in ["Pink", "zqxa", "zqxb"] {
        assert!(!report.contains(leaked), "{leaked:?} leaked:\n{report}");
    }
}
