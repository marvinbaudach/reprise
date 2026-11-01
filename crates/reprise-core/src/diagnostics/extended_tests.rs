//! The report's enabled-modules line and log tails.

use super::{render_report, DiagnosticFacts, DiagnosticLog, RedactionContext};

fn render(facts: &DiagnosticFacts) -> String {
    render_report(
        facts,
        &DiagnosticLog::default(),
        &RedactionContext::default(),
    )
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
fn crash_report_never_contains_a_library_path_in_the_log_tails() {
    let facts = DiagnosticFacts {
        log_tail: Some(
            "INFO reprise: opened /home/tester/Music/Artist/Album/01 Song.flac\n\
             WARN reprise: gone /mnt/nas/Other Album/02.ogg token=hunter2"
                .into(),
        ),
        previous_log_tail: Some(
            "ERROR reprise: panicked reading /home/tester/Music/Artist/Album/03 Third.flac for tester"
                .into(),
        ),
        ..DiagnosticFacts::default()
    };
    let context = RedactionContext {
        music_dir: Some("/home/tester/Music".into()),
        home_dir: Some("/home/tester".into()),
        username: Some("tester".into()),
    };

    let report = render_report(&facts, &DiagnosticLog::default(), &context);

    for leaked in [
        "Artist",
        "Song.flac",
        "Third.flac",
        "Other Album",
        "/mnt/nas",
        "/home/tester",
        "hunter2",
        "tester",
    ] {
        assert!(
            !report.contains(leaked),
            "{leaked:?} leaked into:\n{report}"
        );
    }
    assert!(report.contains("opened"));
    assert!(report.contains("panicked reading"));
}
