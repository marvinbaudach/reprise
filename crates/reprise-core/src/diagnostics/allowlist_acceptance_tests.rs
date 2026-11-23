//! Acceptance of the allowlist redactor: whatever text a user's library puts
//! into a log call, the safe line carries none of it.
//!
//! `testdata/leak_corpus.txt` holds the inputs: shapes that earlier redactors
//! failed on (quoted media names before a colon, slash forms with a number-led
//! second segment, backslash roots, secret words next to values, forged
//! placeholders and forged log lines), every one carrying the needle `zqx` and
//! no real name. Each input is put in every position a value can reach the
//! report through, and none of them may show the needle, with the single
//! exception the invariant names: a plain identifier, an option or tuple
//! variant or a list of those under a `KEEP_TEXT_FIELDS` key (invariant item
//! 3). Under every other `KEEP_FIELDS` key only numbers and bools are shown.
//!
//! The invariant fuzz lives in `allowlist_fuzz_tests`.

use std::collections::BTreeSet;

use super::crash_reduce::{reduce_message, PanicPayload};
use super::field_policy::{KEEP_FIELDS, KEEP_TEXT_FIELDS, REDACT_FIELDS};
use super::field_values::{render_field, RecordedField, RecordedValue};
use super::log_template::{LogTemplate, HOLE};
use super::log_templates::{is_own_file, templates_for_file};
use super::panic_phrases::phrases;
use super::safe_event::{one_line, safe_event, Callsite, LogLevel, RecordedEvent, SafeEvent};

const CORPUS: &str = include_str!("testdata/leak_corpus.txt");
const TABLE: &str = include_str!("log_templates.txt");
const NEEDLE: &str = "zqx";
const FIXTURE_USER: &str = "zqxuser";
const CORPUS_SIZE: std::ops::RangeInclusive<usize> = 150..=250;

/// A source file of an own call site that has templates.
const OWN_FILE: &str = "crates/reprise-gnome/src/ui/sidebar/sidebar_rebuild.rs";
/// A dependency's source file: the template table does not know it.
const FOREIGN_FILE: &str = "/cargo/registry/src/dependency-1.0.0/src/lib.rs";
/// A key of `KEEP_TEXT_FIELDS` that carries state names.
const KEPT_KEY: &str = "state";

/// What a plain identifier, an option or tuple variant, or a list of those
/// shows under a `KEEP_TEXT_FIELDS` key: the corpus inputs of that shape, which
/// the invariant lets through. Written out, not derived, so that a widening of the
/// value shape check changes this list and is seen.
const KEEP_SURVIVORS: &[&str] = &["Some(Zqxa)", "Track(Zqxa)", "[Zqxa, Zqxb, Zqxc]", "zqxuser"];

fn corpus() -> Vec<String> {
    CORPUS
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(decode)
        .collect()
}

/// The corpus file keeps one input per line; these markers stand for the
/// characters that would break that.
fn decode(line: &str) -> String {
    [
        ("<NL>", "\n"),
        ("<CR>", "\r"),
        ("<TAB>", "\t"),
        ("<NBSP>", "\u{a0}"),
        ("<ZWSP>", "\u{200b}"),
        ("<LS>", "\u{2028}"),
    ]
    .iter()
    .fold(line.to_owned(), |text, (marker, ch)| {
        text.replace(marker, ch)
    })
}

fn leaks(text: &str) -> bool {
    text.to_lowercase().contains(NEEDLE)
}

fn callsite(file: Option<&'static str>) -> Callsite {
    Callsite {
        file,
        line: Some(30),
        target: "reprise::ui::sidebar",
        level: LogLevel::Warn,
    }
}

fn message(text: &str) -> RecordedEvent {
    RecordedEvent {
        message: Some(text.to_owned()),
        fields: Vec::new(),
    }
}

/// Every own source file and its templates, the file names made `'static` the
/// way a call site's are.
fn own_files() -> Vec<(&'static str, &'static [LogTemplate])> {
    let files = TABLE
        .lines()
        .filter_map(|line| line.split_once('\t').map(|(file, _)| file))
        .collect::<BTreeSet<_>>();
    files
        .into_iter()
        .map(|file| {
            let file: &'static str = Box::leak(file.to_owned().into_boxed_str());
            (file, templates_for_file(file))
        })
        .collect()
}

/// What a message of `templates` may show: one of their reduced forms or `…`.
fn allowed_messages(templates: &[LogTemplate]) -> BTreeSet<String> {
    templates
        .iter()
        .map(|template| one_line(&template.reduced()))
        .chain([HOLE.to_owned()])
        .collect()
}

#[test]
fn the_corpus_is_a_fixture_of_the_agreed_size_and_every_input_carries_the_needle() {
    let inputs = corpus();

    assert!(
        CORPUS_SIZE.contains(&inputs.len()),
        "the corpus holds {} inputs",
        inputs.len()
    );
    let distinct = inputs.iter().collect::<BTreeSet<_>>();
    assert_eq!(distinct.len(), inputs.len(), "an input is listed twice");
    for input in &inputs {
        assert!(
            leaks(input),
            "no needle in {input:?}: it would test nothing"
        );
        for (at, _) in input.match_indices("/home/") {
            assert!(
                input[at + "/home/".len()..].starts_with(FIXTURE_USER),
                "{input:?} names a user other than the fixture user"
            );
        }
    }
}

#[test]
fn the_table_is_what_the_acceptance_reads_not_a_stand_in() {
    let files = own_files();

    assert!(files.len() > 50, "{} files", files.len());
    assert!(is_own_file(OWN_FILE));
    assert!(files
        .iter()
        .any(|(_, templates)| templates.iter().any(|t| t.hole_count() > 0)));
}

#[test]
fn an_input_as_a_hole_value_of_any_template_shows_only_the_static_template() {
    let inputs = corpus();
    let mut checked = 0_usize;
    for (file, templates) in own_files() {
        let allowed = allowed_messages(templates);
        for template in templates
            .iter()
            .filter(|template| template.hole_count() > 0)
        {
            for input in &inputs {
                let rendered = template.rendered_with(input);
                let event = safe_event(&callsite(Some(file)), &message(&rendered));

                assert!(
                    allowed.contains(event.body()),
                    "{file}: {:?} became {:?}",
                    template.canonical(),
                    event.body()
                );
                assert!(!leaks(event.body()), "{file}: {:?}", event.body());
                assert!(!leaks(&event.line()), "{file}: {:?}", event.line());
                checked += 1;
            }
        }
    }
    assert!(checked > 10_000, "only {checked} combinations ran");
}

#[test]
fn an_input_as_an_unmatched_message_of_an_own_call_site_is_an_ellipsis() {
    for input in corpus() {
        let event = safe_event(&callsite(Some(OWN_FILE)), &message(&input));

        assert_eq!(event.body(), HOLE, "{input:?}");
        assert!(!leaks(&event.line()), "{input:?}");
        assert_eq!(event.line().matches('\n').count(), 1, "{input:?}");
    }
}

#[test]
fn an_input_logged_by_a_foreign_call_site_shows_its_level_and_target_only() {
    let leaked: Vec<&'static str> = corpus()
        .into_iter()
        .map(|input| &*Box::leak(input.into_boxed_str()))
        .collect();
    for input in leaked {
        let recorded = RecordedEvent {
            message: Some(input.to_owned()),
            fields: vec![
                RecordedField::new(KEPT_KEY, RecordedValue::Str(input.to_owned())),
                RecordedField::new("path", RecordedValue::Debug(input.to_owned())),
            ],
        };
        // A foreign file, no file at all, and a file named like the input.
        for file in [Some(FOREIGN_FILE), None, Some(input)] {
            let event = safe_event(&callsite(file), &recorded);

            assert_eq!(event.body(), HOLE, "{input:?}");
            assert_eq!(event.location(), None, "{input:?}");
            assert!(!leaks(&event.line()), "{input:?}");
        }
    }
}

/// The line of an own event whose message and fields all carry `input`.
fn own_line_carrying(input: &str) -> SafeEvent {
    let recorded = RecordedEvent {
        message: Some(input.to_owned()),
        fields: vec![
            RecordedField::new(KEPT_KEY, RecordedValue::Str(input.to_owned())),
            RecordedField::new("title", RecordedValue::Str(input.to_owned())),
            RecordedField::new("path", RecordedValue::Debug(input.to_owned())),
            RecordedField::new("count", RecordedValue::Int(3)),
        ],
    };
    safe_event(&callsite(Some(OWN_FILE)), &recorded)
}

#[test]
fn an_input_in_every_field_of_an_own_event_leaves_at_most_a_plain_kept_value() {
    for input in corpus() {
        let line = own_line_carrying(&input).line();
        let without_kept_value = line.replace(&format!("{KEPT_KEY}={input}"), "");

        assert!(!leaks(&without_kept_value), "{input:?}: {line:?}");
        assert!(
            line.contains("title=$REDACTED path=$REDACTED count=3"),
            "{line:?}"
        );
        assert!(
            line.contains(&format!("; {KEPT_KEY}=")),
            "the message is followed by the fields: {line:?}"
        );
    }
}

#[test]
fn an_input_as_a_formatted_panic_payload_is_matched_or_an_ellipsis() {
    let inputs = corpus();
    let mut messages = Vec::new();
    for input in &inputs {
        messages.push(input.clone());
        // The input as the value inside each standard phrase.
        messages.extend(phrases().iter().map(|phrase| phrase.rendered_with(input)));
    }
    for message in messages {
        let shown = reduce_message(&PanicPayload::Formatted(message.clone()), None);

        assert!(!leaks(&shown), "{message:?} became {shown:?}");
        assert!(!shown.contains('\n'), "{shown:?}");
    }
}

#[test]
fn an_input_inside_a_template_of_the_panic_location_file_is_reduced_with_it() {
    let inputs = corpus();
    let mut checked = 0_usize;
    for (file, templates) in own_files() {
        for template in templates
            .iter()
            .filter(|template| template.hole_count() > 0)
        {
            for input in &inputs {
                let message = template.rendered_with(input);
                let shown = reduce_message(&PanicPayload::Formatted(message), Some(file));

                assert!(
                    !leaks(&shown),
                    "{file}: {:?} became {shown:?}",
                    template.canonical()
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 10_000, "only {checked} combinations ran");
}

/// The corpus inputs whose needle shows in `key`'s rendering, for a `Str` and
/// for a `Debug` value alike.
fn survivors_under(key: &'static str, inputs: &[String]) -> BTreeSet<String> {
    let mut shown = BTreeSet::new();
    for kind in [
        |input: &str| RecordedValue::Str(input.to_owned()),
        |input: &str| RecordedValue::Debug(input.to_owned()),
    ] {
        let survivors = inputs
            .iter()
            .filter(|input| leaks(&render_field(key, &kind(input))))
            .cloned()
            .collect::<BTreeSet<_>>();
        if shown.is_empty() {
            shown = survivors;
        } else {
            assert_eq!(shown, survivors, "key {key}: Str and Debug differ");
        }
    }
    shown
}

#[test]
fn an_input_as_the_text_of_a_text_key_survives_only_as_a_plain_shape() {
    let inputs = corpus();
    let expected = KEEP_SURVIVORS
        .iter()
        .map(|survivor| (*survivor).to_owned())
        .collect::<BTreeSet<_>>();
    for key in KEEP_TEXT_FIELDS {
        assert_eq!(survivors_under(key, &inputs), expected, "key {key}");
    }
    // What survives is the value as written, plain by its whole shape.
    for survivor in KEEP_SURVIVORS {
        let shown = render_field(KEPT_KEY, &RecordedValue::Str((*survivor).to_owned()));
        assert_eq!(shown, format!("{KEPT_KEY}={survivor}"));
        assert!(
            survivor
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || "_()[], ".contains(ch)),
            "{survivor}"
        );
    }
}

#[test]
fn an_input_as_the_text_of_any_other_kept_key_is_always_redacted() {
    let inputs = corpus();
    let numeric_keys = KEEP_FIELDS
        .iter()
        .filter(|key| KEEP_TEXT_FIELDS.binary_search(key).is_err())
        .collect::<Vec<_>>();

    assert!(numeric_keys.len() > 100, "{} keys", numeric_keys.len());
    for key in numeric_keys {
        assert_eq!(survivors_under(key, &inputs), BTreeSet::new(), "key {key}");
        // The fixture user as a plain word is the case the subset exists for.
        for value in [
            RecordedValue::Str(FIXTURE_USER.to_owned()),
            RecordedValue::Debug(FIXTURE_USER.to_owned()),
        ] {
            assert_eq!(render_field(key, &value), format!("{key}=$REDACTED"));
        }
    }
}

#[test]
fn an_input_as_the_text_of_a_field_that_is_not_kept_is_always_redacted() {
    for input in corpus() {
        for key in REDACT_FIELDS {
            for value in [
                RecordedValue::Str(input.clone()),
                RecordedValue::Debug(input.clone()),
            ] {
                assert_eq!(render_field(key, &value), format!("{key}=$REDACTED"));
            }
        }
    }
}
