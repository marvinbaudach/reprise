//! The value-shape rules for typed fields: what a kept key may carry, by kind.
//!
//! The cases come from the retired free-text pass's tests and from the file
//! mirror's contract test. A value is judged exactly as written, so a typed
//! value that carries running-text punctuation or a second token is redacted.

use super::field_values::{
    is_kept_text, render_field, RecordedField, RecordedValue, MAX_VALUE_BYTES, REDACTED,
};

fn text(value: &str) -> [RecordedValue; 2] {
    [
        RecordedValue::Str(value.to_owned()),
        RecordedValue::Debug(value.to_owned()),
    ]
}

/// The field as written when every text kind keeps it, else `None`.
fn kept_as_text(key: &'static str, value: &str) -> Option<String> {
    let [string, debug] = text(value).map(|value| render_field(key, &value));
    assert_eq!(string, debug, "`{key}={value:?}` reads differently by kind");
    (!string.ends_with(REDACTED)).then_some(string)
}

fn assert_kept(key: &'static str, value: &str) {
    assert_eq!(
        kept_as_text(key, value),
        Some(format!("{key}={value}")),
        "{key}={value:?}"
    );
}

fn assert_redacted(key: &'static str, value: &str) {
    assert_eq!(
        kept_as_text(key, value),
        None,
        "{key}={value:?} must be redacted"
    );
    assert_eq!(
        render_field(key, &RecordedValue::Str(value.to_owned())),
        format!("{key}={REDACTED}"),
        "{key}={value:?}"
    );
}

#[test]
fn a_kept_key_keeps_a_typed_number_or_bool_as_written() {
    for (field, expected) in [
        (
            RecordedField::new("elapsed_ms", RecordedValue::Int(12)),
            "elapsed_ms=12",
        ),
        (
            RecordedField::new("count", RecordedValue::Int(-3)),
            "count=-3",
        ),
        (
            RecordedField::new("track_id", RecordedValue::Uint(u128::from(u64::MAX))),
            "track_id=18446744073709551615",
        ),
        (
            RecordedField::new("track_id", RecordedValue::Int(i128::MIN)),
            "track_id=-170141183460469231731687303715884105728",
        ),
        (
            RecordedField::new("placeholder_distance", RecordedValue::Float(0.25)),
            "placeholder_distance=0.25",
        ),
        (
            RecordedField::new("elapsed_ms", RecordedValue::Float(1.0)),
            "elapsed_ms=1.0",
        ),
        (
            RecordedField::new("is_shuffled", RecordedValue::Bool(true)),
            "is_shuffled=true",
        ),
    ] {
        assert_eq!(render_field(field.key, &field.value), expected);
    }
}

#[test]
fn a_key_that_is_not_kept_is_redacted_whatever_its_kind_or_value() {
    for value in [
        RecordedValue::Int(7),
        RecordedValue::Uint(7),
        RecordedValue::Float(0.5),
        RecordedValue::Bool(false),
        RecordedValue::Str("Playing".to_owned()),
        RecordedValue::Debug("Playing".to_owned()),
    ] {
        for key in ["volume", "error", "failure_message", "path", "zqx_unlisted"] {
            assert_eq!(
                render_field(key, &value),
                format!("{key}={REDACTED}"),
                "{key} {value:?}"
            );
        }
    }
}

#[test]
fn a_kept_key_keeps_a_plain_identifier_or_a_number_as_text() {
    for (key, value) in [
        ("mode", "Compact"),
        ("state", "Playing"),
        ("operation", "Track7"),
        ("kind", "_private"),
        ("count", "3"),
        ("count", "-3"),
        ("count", "1.5"),
        ("count", "12"),
        ("id", "0.25"),
        ("version", "0.1.331"),
        ("version", "1.2.3.4"),
        ("state", "None"),
        ("state", "true"),
    ] {
        assert_kept(key, value);
    }
}

#[test]
fn a_kept_key_never_carries_a_name_a_path_or_a_second_value_through() {
    for (key, value) in [
        ("elapsed_ms", "12;file=Artist"),
        ("count", "3;title=Foo"),
        ("mode", "Compact,title=Foo"),
        ("version", "0.1.331;title=Foo"),
        ("version", "0.1.331-Artist"),
        ("id", "2024-11-08-Artist"),
        ("id", "deadbeefcafe"),
        ("kind", "Music/Artist/Album"),
        ("state", "/home/zqxuser/song.flac"),
        ("state", "song.flac"),
        ("state", "~/x"),
        ("state", "./x"),
        ("state", ""),
        ("state", "\"Zqx Floyd/The Wall\""),
        ("state", "Zqx(Floyd The Wall)"),
        ("state", "(Zqx Floyd)"),
        ("mode", "a/b next words"),
        ("kind", "Zqx Floyd The Wall"),
        ("count", "song.mp3"),
    ] {
        assert_redacted(key, value);
    }
}

#[test]
fn more_than_one_dot_is_a_version_only_under_the_version_key() {
    for (key, value) in [
        ("count", "192.168.1.5"),
        ("id", "1.1.1.1"),
        ("elapsed_ms", "0.1.331"),
    ] {
        assert_redacted(key, value);
    }
    for (key, value) in [("count", "1.5"), ("id", "0.25"), ("version", "0.1.331")] {
        assert_kept(key, value);
    }
}

#[test]
fn a_debug_number_keeps_its_key_and_value() {
    for value in [
        "Some(1.5)",
        "Some(250)",
        "None",
        "12.5ms",
        "3.5µs",
        "40us",
        "100ns",
        "2s",
        "Some(12.5ms)",
        "1.5e16",
        "1e-3",
        "2E+4",
        "-1.5e-3",
    ] {
        assert_kept("elapsed_ms", value);
    }
}

#[test]
fn something_that_only_looks_like_a_debug_number_is_redacted() {
    for value in [
        "Some(Artist.flac)",
        "Some(1.5",
        "1.5.5ms",
        "1.e5",
        "1e",
        "12.5msx",
        "Some(1)(2)",
    ] {
        assert_redacted("elapsed_ms", value);
    }
}

#[test]
fn an_option_of_a_plain_value_keeps_its_key_and_value() {
    for (key, value) in [
        ("current_up_next", "Some(Track(5))"),
        ("current_up_next", "Some(Episode(12))"),
        ("current_up_next", "None"),
        ("state", "Some(7)"),
        ("state", "Some(Playing)"),
        ("state", "Some(true)"),
        ("state", "Some(Some(Some(1)))"),
        ("current_up_next", "Some(Track(-5))"),
    ] {
        assert_kept(key, value);
    }
}

#[test]
fn a_list_of_integers_keeps_its_key_and_value() {
    for value in ["[]", "[1]", "[1, 2, 3]", "[-1, 20]"] {
        assert_kept("episode_ids", value);
    }
}

#[test]
fn a_list_of_plain_words_is_kept_only_under_a_text_key() {
    for value in [
        "[NowPlaying, PlayNext(0), UpNext(1)]",
        "[Some(1), None]",
        "[1, 2, 3]",
    ] {
        assert_kept("rows", value);
    }
    for value in ["[NowPlaying, PlayNext(0), UpNext(1)]", "[Some(1), None]"] {
        assert_redacted("episode_ids", value);
    }
}

#[test]
fn a_plain_word_under_a_key_that_carries_numbers_is_redacted() {
    // A kept key that is not in `KEEP_TEXT_FIELDS` carries ids and counts, so
    // a word under it is foreign text that merely looks like a variant name.
    for (key, value) in [
        ("id", "marvin"),
        ("count", "zqxuser"),
        ("elapsed_ms", "Playing"),
        ("track_id", "_private"),
        ("id", "Some(marvin)"),
        ("id", "Track(5)"),
        ("episode_ids", "[marvin]"),
        ("episode_ids", "Some(Track(5))"),
        ("current_pos", "[marvin, bob]"),
    ] {
        assert_redacted(key, value);
    }
    // A text key keeps the same word, and every key keeps the fixed literals.
    assert_kept("state", "marvin");
    for key in ["id", "count", "elapsed_ms", "state"] {
        for value in ["None", "true", "false", "Some(7)", "[1, 2]"] {
            assert_kept(key, value);
        }
    }
}

#[test]
fn a_list_separated_by_anything_but_a_comma_and_one_space_is_redacted() {
    for value in ["[1,\n2]", "[1,  2]", "[1,\t2]", "[1 , 2]", "[1,2]"] {
        assert_redacted("episode_ids", value);
        assert_redacted("rows", value);
    }
}

#[test]
fn a_list_at_the_element_bound_survives_and_one_more_does_not() {
    let list = |count: usize| {
        let elements = (0..count)
            .map(|index| index.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        format!("[{elements}]")
    };

    assert_kept("episode_ids", &list(64));
    assert_redacted("episode_ids", &list(65));
    assert_redacted("episode_ids", &list(100));
}

#[test]
fn a_composite_value_with_anything_but_plain_parts_is_redacted() {
    for (key, value) in [
        ("current_up_next", "Some(\"Zqx Floyd\")"),
        ("current_up_next", "Some(/home/zqxuser)"),
        ("episode_ids", "[1, /home/zqxuser]"),
        ("episode_ids", "[Zqx Name]"),
        ("current_up_next", "Some(Track(deadbeefcafe))"),
        ("episode_ids", "[1, 2"),
        ("episode_ids", "1, 2]"),
        ("state", "Some(Some(Some(Some(1))))"),
        ("episode_ids", "[[1], [2]]"),
        ("episode_ids", "[1, ]"),
        ("episode_ids", "[1.5, 2]"),
        ("episode_ids", "[a.flac, b]"),
        ("episode_ids", "[a:b]"),
        ("state", "Some(Track(1.5))"),
        ("state", "Some(Track())"),
        ("state", "Some()"),
    ] {
        assert_redacted(key, value);
    }
}

#[test]
fn a_struct_like_value_is_redacted() {
    for (key, value) in [
        ("rows", "QueueRow { index: 1 }"),
        ("rows", "[QueueRow { index: 1 }]"),
        ("current_up_next", "Some(Track { id: 5 })"),
    ] {
        assert_redacted(key, value);
    }
}

#[test]
fn a_value_is_judged_exactly_as_written_so_running_text_punctuation_is_not_kept() {
    // A typed value is the whole value: trailing `,` `;` `:` `]` `}` `"` or a
    // wrapping `)` makes it text.
    for (key, value) in [
        ("count", "3,"),
        ("count", "(3)"),
        ("count", "3;"),
        ("state", "Playing,"),
        ("state", "Playing "),
        ("state", " Playing"),
        ("state", "Playing\n"),
        ("elapsed_ms", "Some(1.5),"),
        ("current_up_next", "Some(Track(5)),"),
        ("episode_ids", "[1, 2]x"),
        ("episode_ids", "[1, 2],"),
        ("episode_ids", "[1, 2]]"),
        ("id", "$USER"),
    ] {
        assert_redacted(key, value);
    }
}

#[test]
fn a_kept_key_with_a_foreign_text_kind_is_judged_by_its_shape_like_the_mirror_contract() {
    for (key, value) in [
        ("version", "0.1.331"),
        ("state", "Playing"),
        ("placeholder_distance", "0.25"),
    ] {
        assert_kept(key, value);
    }
    for (key, value) in [
        ("failure_message", "Zqx - Title could not be decoded"),
        ("error", "could not open /home/zqxuser/Music/Zqx/x.flac"),
        ("volume", "0.5"),
    ] {
        assert_redacted(key, value);
    }
}

#[test]
fn a_value_up_to_the_length_cap_is_judged_by_its_shape_and_one_past_it_is_redacted() {
    let fits = format!("[{}]", vec!["1"; 40].join(", "));
    assert!(fits.len() < MAX_VALUE_BYTES);
    let identifier = |length: usize| "z".repeat(length);

    assert!(is_kept_text("state", &identifier(MAX_VALUE_BYTES)));
    assert!(!is_kept_text("state", &identifier(MAX_VALUE_BYTES + 1)));
    assert!(is_kept_text("episode_ids", &fits));
}

#[test]
fn a_huge_value_under_a_kept_key_is_redacted_quickly() {
    let started = std::time::Instant::now();
    for value in [
        ")".repeat(1_000_000),
        "[".repeat(1_000_000),
        format!("[{}", "1, ".repeat(300_000)),
        "Some(".repeat(200_000),
    ] {
        assert_redacted("state", &value);
    }

    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "took {:?}",
        started.elapsed()
    );
}
