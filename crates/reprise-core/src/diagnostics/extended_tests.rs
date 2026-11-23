//! The report's enabled-modules line and log tails.

use super::{render_report, DiagnosticFacts, DiagnosticLog};

fn render(facts: &DiagnosticFacts) -> String {
    render_report(facts, &DiagnosticLog::default())
}

#[test]
fn fb_19_the_copied_report_lists_the_enabled_modules() {
    let facts = DiagnosticFacts {
        enabled_modules: Some(vec!["mpris".into(), "radio".into()]),
        ..DiagnosticFacts::default()
    };

    assert!(render(&facts).contains("\nmodules mpris radio"));
}

#[test]
fn no_enabled_module_says_none_and_unknown_modules_say_nothing() {
    let none = DiagnosticFacts {
        enabled_modules: Some(Vec::new()),
        ..DiagnosticFacts::default()
    };

    assert!(render(&none).contains("\nmodules none"));
    assert!(!render(&DiagnosticFacts::default()).contains("modules"));
}

#[test]
fn fb_19_the_copied_report_carries_both_runs_log_tails_labelled() {
    let facts = DiagnosticFacts {
        log_tail: Some("INFO this run line".into()),
        previous_log_tail: Some("ERROR crashed run line".into()),
        ..DiagnosticFacts::default()
    };

    let report = render(&facts);

    let this_run = report.find("log, this run\nINFO this run line").unwrap();
    let previous = report
        .find("log, previous run\nERROR crashed run line")
        .unwrap();
    assert!(this_run < previous);
}

#[test]
fn a_report_without_logs_has_no_log_headings() {
    assert!(!render(&DiagnosticFacts::default()).contains("log, "));
}

#[test]
fn fb_19_the_log_tails_are_embedded_line_by_line_as_they_were_written() {
    // The run log holds only safe lines (`safe_event`), so the report copies
    // them without reading them: the timestamp, the `@ file:line` call site
    // and the `…` of a foreign event all arrive as they are.
    let this_run = "2026-11-08T12:00:00.123Z  WARN reprise_core::run_log: could not move the previous log aside; error=$REDACTED @ crates/reprise-core/src/run_log.rs:42\n\
                    2026-11-08T12:00:01.000Z  INFO foreign_crate: …";
    let facts = DiagnosticFacts {
        log_tail: Some(this_run.into()),
        previous_log_tail: Some("2026-11-07T09:00:00.000Z ERROR reprise: panic".into()),
        ..DiagnosticFacts::default()
    };

    let report = render(&facts);

    assert!(
        report.contains(&format!("log, this run\n{this_run}")),
        "{report}"
    );
    assert!(
        report.contains("log, previous run\n2026-11-07T09:00:00.000Z ERROR reprise: panic"),
        "{report}"
    );
}
