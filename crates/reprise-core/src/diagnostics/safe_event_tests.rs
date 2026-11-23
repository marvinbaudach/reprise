//! The safe event: static message templates, kept fields, own versus foreign
//! call sites, one line per event, and the location suffix.

use chrono::{DateTime, TimeZone, Utc};

use super::field_values::{RecordedField, RecordedValue, REDACTED};
use super::log_template::LogTemplate;
use super::log_templates::templates_for_file;
use super::safe_event::{
    foreign_event, one_line, own_event, safe_event, Callsite, LogLevel, RecordedEvent, SafeEvent,
};

const SIDEBAR_FILE: &str = "crates/reprise-gnome/src/ui/sidebar/sidebar_rebuild.rs";

fn at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 22, 10, 0, 0)
        .single()
        .map(|time| time + chrono::Duration::milliseconds(123))
        .unwrap_or_default()
}

fn callsite(file: Option<&'static str>, level: LogLevel) -> Callsite {
    Callsite {
        file,
        line: Some(30),
        target: "reprise::ui::sidebar",
        level,
    }
}

fn templates(formats: &[&str]) -> Vec<LogTemplate> {
    formats
        .iter()
        .map(|format| LogTemplate::parse(format).unwrap_or_else(|error| panic!("{error}")))
        .collect()
}

fn message(text: &str) -> RecordedEvent {
    RecordedEvent {
        message: Some(text.to_owned()),
        fields: Vec::new(),
    }
}

fn with_fields(text: &str, fields: Vec<RecordedField>) -> RecordedEvent {
    RecordedEvent {
        message: Some(text.to_owned()),
        fields,
    }
}

fn own(formats: &[&str], recorded: &RecordedEvent) -> SafeEvent {
    own_event(
        &callsite(Some(SIDEBAR_FILE), LogLevel::Warn),
        SIDEBAR_FILE,
        &templates(formats),
        recorded,
    )
}

#[test]
fn an_own_event_is_its_static_template_its_kept_fields_and_its_location() {
    let recorded = with_fields(
        "could not rebuild the sidebar for Zqx Floyd",
        vec![
            RecordedField::new("track_id", RecordedValue::Int(7)),
            RecordedField::new("error", RecordedValue::Debug("Zqx failure".to_owned())),
        ],
    );

    let event = own(&["could not rebuild the sidebar for {}"], &recorded);

    assert_eq!(
        event.line_at(at()),
        "2026-11-22T10:00:00.123Z  WARN reprise::ui::sidebar: could not rebuild the sidebar for …; \
         track_id=7 error=$REDACTED @ crates/reprise-gnome/src/ui/sidebar/sidebar_rebuild.rs:30\n"
    );
}

#[test]
fn the_real_table_gives_the_message_of_a_known_own_file() {
    let plain = templates_for_file(SIDEBAR_FILE)
        .iter()
        .find(|template| template.hole_count() == 0)
        .map(LogTemplate::reduced)
        .unwrap_or_default();
    assert!(
        !plain.is_empty(),
        "the table has no plain template for {SIDEBAR_FILE}"
    );

    let event = safe_event(
        &callsite(Some(SIDEBAR_FILE), LogLevel::Warn),
        &message(&plain),
    );

    // `reduced()` of a hole-free template is the text the call site renders,
    // braces and line feeds as they are, unlike its `canonical()` format.
    assert_eq!(event.body(), one_line(&plain));
    assert_eq!(
        event.location(),
        Some(format!("{SIDEBAR_FILE}:30").as_str())
    );
}

#[test]
fn a_value_in_a_hole_never_reaches_the_line() {
    let event = own(
        &["could not read {} for {}"],
        &message("could not read /home/zqxuser/Music/Zqx.flac for zqxuser"),
    );

    assert_eq!(event.body(), "could not read … for …");
    assert!(
        !event.line().to_lowercase().contains("zqx"),
        "{}",
        event.line()
    );
}

#[test]
fn a_message_no_template_matches_is_a_placeholder() {
    for text in ["Zqx said hello", "", "could not read"] {
        let event = own(&["could not read {} for {}"], &message(text));

        assert_eq!(event.body(), "…", "{text:?}");
    }
    assert_eq!(own(&[], &message("Zqx")).body(), "…");
}

#[test]
fn the_most_specific_template_wins_and_text_that_only_looks_like_one_does_not() {
    let event = own(
        &["{}", "could not {}", "could not read {}"],
        &message("could not read Zqx"),
    );

    assert_eq!(event.body(), "could not read …");
}

#[test]
fn a_field_of_an_own_event_that_the_policy_does_not_keep_is_redacted() {
    let recorded = with_fields(
        "playback stalled",
        vec![
            RecordedField::new("elapsed_ms", RecordedValue::Int(12)),
            RecordedField::new("placeholder_distance", RecordedValue::Float(0.25)),
            RecordedField::new("version", RecordedValue::Str("0.1.331".to_owned())),
            RecordedField::new("state", RecordedValue::Debug("Playing".to_owned())),
            RecordedField::new("volume", RecordedValue::Float(0.5)),
            RecordedField::new(
                "path",
                RecordedValue::Str("/home/zqxuser/Zqx.flac".to_owned()),
            ),
            RecordedField::new("mode", RecordedValue::Str("Zqx Floyd".to_owned())),
            RecordedField::new("kind", RecordedValue::Str("Zqx/Floyd".to_owned())),
        ],
    );

    let event = own(&["playback stalled"], &recorded);

    assert_eq!(
        event.body(),
        format!(
            "playback stalled; elapsed_ms=12 placeholder_distance=0.25 version=0.1.331 \
             state=Playing volume={REDACTED} path={REDACTED} mode={REDACTED} kind={REDACTED}"
        )
    );
}

#[test]
fn an_event_without_a_message_shows_its_fields_or_says_it_has_no_details() {
    let fields = RecordedEvent {
        message: None,
        fields: vec![RecordedField::new("count", RecordedValue::Uint(3))],
    };

    assert_eq!(own(&["x"], &fields).body(), "count=3");
    assert_eq!(
        own(&["x"], &RecordedEvent::default()).body(),
        "warning without details"
    );
}

#[test]
fn a_foreign_event_is_its_level_target_and_a_placeholder_whatever_it_carries() {
    let recorded = with_fields(
        "connect to zqxhost.example failed for zqxuser",
        vec![
            RecordedField::new("count", RecordedValue::Int(3)),
            RecordedField::new("state", RecordedValue::Str("Zqx".to_owned())),
        ],
    );
    for file in [
        None,
        Some("/home/zqxuser/.cargo/registry/src/index-1/zqx-1.0.0/src/lib.rs"),
        Some("crates/reprise-gnome/src/ui/not_in_the_table.rs"),
    ] {
        let event = safe_event(
            &Callsite {
                file,
                line: Some(9),
                target: "zqx_dependency::net",
                level: LogLevel::Info,
            },
            &recorded,
        );

        assert_eq!(
            event.line_at(at()),
            "2026-11-22T10:00:00.123Z  INFO zqx_dependency::net: …\n",
            "{file:?}"
        );
        assert_eq!(event.location(), None);
        assert_eq!(event.suffix(), "");
    }
}

#[test]
fn a_bridged_log_record_has_no_file_and_is_foreign() {
    let event = foreign_event(&Callsite {
        file: None,
        line: None,
        target: "log",
        level: LogLevel::Error,
    });

    assert_eq!(
        event.line_at(at()),
        "2026-11-22T10:00:00.123Z ERROR log: …\n"
    );
}

#[test]
fn a_line_feed_in_a_template_is_written_as_two_characters() {
    // The shape of a real template of the gnome layer: `SCROLL JUMP-TO-TOP\n{}`.
    let event = own(
        &["SCROLL JUMP-TO-TOP\n{}"],
        &message("SCROLL JUMP-TO-TOP\nZqx Floyd"),
    );
    let line = event.line_at(at());

    assert_eq!(event.body(), "SCROLL JUMP-TO-TOP\\n…");
    assert_eq!(line.matches('\n').count(), 1, "{line:?}");
    assert!(line.ends_with('\n'), "{line:?}");
}

#[test]
fn a_line_break_in_a_hole_or_a_template_never_splits_the_line() {
    let event = own(&["a\r\n{}\nb"], &message("a\r\nZqx\nb"));

    assert_eq!(event.body(), "a\\r\\n…\\nb");
    assert_eq!(event.line().matches(['\n', '\r']).count(), 1);
}

#[test]
fn the_location_is_the_file_alone_when_the_site_has_no_line() {
    let event = own_event(
        &Callsite {
            line: None,
            ..callsite(Some(SIDEBAR_FILE), LogLevel::Warn)
        },
        SIDEBAR_FILE,
        &templates(&["x"]),
        &message("x"),
    );

    assert_eq!(event.suffix(), format!(" @ {SIDEBAR_FILE}"));
}

#[test]
fn the_target_segment_is_the_last_path_segment_and_a_bare_target_stays_whole() {
    let event = own(&["could not read {}"], &message("could not read Zqx"));
    assert_eq!(event.target_segment(), "sidebar");

    let foreign = foreign_event(&Callsite {
        file: None,
        line: None,
        target: "zbus",
        level: LogLevel::Warn,
    });
    assert_eq!(foreign.target_segment(), "zbus");
    assert_eq!(foreign.suffix(), "");
}

#[test]
fn every_level_is_written_right_aligned_to_five_columns() {
    for (level, padded) in [
        (LogLevel::Trace, "TRACE"),
        (LogLevel::Debug, "DEBUG"),
        (LogLevel::Info, " INFO"),
        (LogLevel::Warn, " WARN"),
        (LogLevel::Error, "ERROR"),
    ] {
        let event = foreign_event(&callsite(None, level));

        assert!(
            event.line_at(at()).contains(&format!("Z {padded} ")),
            "{level:?}"
        );
    }
}

#[test]
fn the_line_of_now_starts_with_an_rfc3339_time_and_ends_with_the_body() {
    let event = own(&["x {}"], &message("x Zqx"));
    let line = event.line();

    assert!(line.starts_with("20"), "{line}");
    assert_eq!(
        line.find('Z'),
        Some("2026-11-22T10:00:00.123".len()),
        "{line}"
    );
    assert!(
        line.ends_with(&format!("x … @ {SIDEBAR_FILE}:30\n")),
        "{line}"
    );
}

#[test]
fn needles_in_every_position_never_survive() {
    let needles = [
        "Zqx",
        "zqxuser",
        "/home/zqxuser/Music/Zqx.flac",
        "password: zqx",
        "Zqx Floyd/The Wall",
    ];
    for needle in needles {
        let recorded = with_fields(
            &format!("opened {needle} for {needle}"),
            vec![
                RecordedField::new("error", RecordedValue::Str(needle.to_owned())),
                RecordedField::new("state", RecordedValue::Debug(needle.to_owned())),
                RecordedField::new("count", RecordedValue::Str(needle.to_owned())),
            ],
        );
        for event in [
            own(&["opened {} for {}"], &recorded),
            own(&["nothing like it"], &recorded),
            foreign_event(&callsite(None, LogLevel::Warn)),
        ] {
            // The one value that may stay is a single plain identifier under a
            // kept key (the policy's residual): take that out, nothing is left.
            let shown = event
                .line()
                .to_lowercase()
                .replace(&format!("state={}", needle.to_lowercase()), "")
                .replace(&format!("count={}", needle.to_lowercase()), "");

            assert!(!shown.contains("zqx"), "{needle:?} reached {shown}");
        }
    }
}
