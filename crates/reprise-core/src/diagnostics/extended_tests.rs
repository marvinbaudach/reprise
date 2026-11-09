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

fn redacted(message: &str) -> String {
    super::redact_log_message(message, &RedactionContext::default())
}

#[test]
fn a_kept_field_with_a_float_or_a_version_arrives_verbatim() {
    assert_eq!(
        redacted("playback stalled elapsed_ms=12.5 version=0.1.331 placeholder_distance=0.25 offset=-1.5"),
        "playback stalled elapsed_ms=12.5 version=0.1.331 placeholder_distance=0.25 offset=-1.5"
    );
}

#[test]
fn a_filename_is_still_redacted_with_or_without_a_key() {
    for (input, expected) in [
        ("opened cover.jpg now", "opened … now"),
        ("opened track01.flac now", "opened … now"),
        ("opened count=song.mp3 now", "opened count=$REDACTED now"),
        ("opened count=1.5.mp3 now", "opened count=$REDACTED now"),
        ("opened count=v1.2 now", "opened count=$REDACTED now"),
    ] {
        assert_eq!(redacted(input), expected, "{input}");
    }
}

#[test]
fn a_redacted_key_stays_redacted_whatever_its_value_looks_like() {
    assert_eq!(
        redacted("volume=0.5 path=0.1.331"),
        "volume=$REDACTED path=$REDACTED"
    );
}

#[test]
fn a_colon_before_the_equals_sign_never_exempts_a_filename() {
    // The key pass ends the key at the first `=` or `:`, so these tokens are
    // not `key=value` to it and only the filename pass can redact them.
    for input in [
        "name:cover.jpg=1.2",
        "a:b=0.25",
        "x:elapsed_ms=1.5",
        "track:song.mp3=0.1.331",
        ":cover.jpg=1.2",
    ] {
        let output = redacted(&format!("opened {input} now"));

        assert_eq!(output, "opened … now", "{input}");
    }
}

#[test]
fn only_a_kept_key_exempts_a_number_from_the_filename_pass() {
    // `cover.jpg` is no kept key, so the key pass redacts the value before the
    // filename pass runs; the number is not what protects it.
    assert_eq!(redacted("cover.jpg=1.2"), "cover.jpg=$REDACTED");
    assert_eq!(redacted("(elapsed_ms=1.5),"), "(elapsed_ms=1.5),");
}

#[test]
fn a_log_tail_line_keeps_its_rfc3339_timestamp_in_the_copied_report() {
    // The file mirror starts every line with `SecondsFormat::Millis` in UTC.
    // `.123Z` has the shape of a file extension; the report re-redacts the
    // tail and must not turn the time into `…`.
    let facts = DiagnosticFacts {
        log_tail: Some(
            "2026-11-08T12:00:00.123Z  WARN reprise: playback stalled elapsed_ms=12\n\
             2026-11-08T12:00:01+01:00  INFO reprise: resumed\n\
             2026-11-08T12:00:02.5-05:30  INFO reprise: resumed again"
                .into(),
        ),
        ..DiagnosticFacts::default()
    };

    let report = render(&facts);

    for line in [
        "\n2026-11-08T12:00:00.123Z  WARN reprise: playback stalled elapsed_ms=12",
        "\n2026-11-08T12:00:01+01:00  INFO reprise: resumed",
        "\n2026-11-08T12:00:02.5-05:30  INFO reprise: resumed again",
    ] {
        assert!(report.contains(line), "timestamp lost: {report}");
    }
}

#[test]
fn only_a_strict_rfc3339_timestamp_escapes_the_filename_pass() {
    for input in [
        "2026-11-08T12:00:00.123Zx",
        "2026-11-08T12:00:00.123Z.flac",
        "2026-11-08 12:00:00.123Z",
        "2026-11-08T12:00.123Z",
        "26-11-08T12:00:00.123Z",
        "2026-11-08T12:00:00.Z",
        "2026-11-08t12:00:00.123z",
    ] {
        let output = redacted(&format!("at {input} done"));

        assert!(
            !output.contains("123Z") && !output.contains("00.Z"),
            "{input}: {output}"
        );
    }
}
