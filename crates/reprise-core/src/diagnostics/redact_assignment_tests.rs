//! The `key=value` pass in free text: what a kept key may carry (M1, L1, L2)
//! and how far a redacted key's value reaches (P3).

use super::{redact_log_message, RedactionContext};

fn redacted(message: &str) -> String {
    redact_log_message(message, &RedactionContext::default())
}

#[test]
fn a_kept_key_never_carries_a_second_assignment_or_a_name_through() {
    for (input, expected) in [
        ("elapsed_ms=12;file=Artist", "elapsed_ms=$REDACTED"),
        ("count=3;title=Foo", "count=$REDACTED"),
        ("mode=Compact,title=Foo", "mode=$REDACTED"),
        ("version=0.1.331;title=Foo", "version=$REDACTED"),
        ("version=0.1.331-Artist", "version=$REDACTED"),
        ("id=2024-11-08-Artist", "id=$REDACTED"),
        ("id=deadbeefcafe", "id=$REDACTED"),
        ("kind=Music/Artist/Album", "kind=$REDACTED"),
        ("state=/home/x/song.flac", "state=$REDACTED"),
        ("state=song.flac", "state=$REDACTED"),
        ("state=~/x", "state=$REDACTED"),
        ("state=./x", "state=$REDACTED"),
        ("state=", "state=$REDACTED"),
    ] {
        assert_eq!(redacted(input), expected, "{input}");
    }
}

#[test]
fn a_kept_key_keeps_a_plain_identifier_or_a_number() {
    for input in [
        "mode=Compact",
        "state=Playing",
        "id=Track7",
        "kind=_private",
        "count=3",
        "count=-3",
        "count=3,",
        "(count=3)",
        "[count=3]",
        "version=0.1.331",
        "version=1.2.3.4",
        "mode=Compact; id=12 kind=Music",
    ] {
        assert_eq!(redacted(input), input, "{input}");
    }
}

#[test]
fn a_kept_key_with_a_rejected_value_takes_the_words_after_it_like_an_unknown_key() {
    for (input, expected) in [
        ("opened count=song.mp3 now", "opened count=$REDACTED"),
        ("kind=Music/Pink Floyd/The Wall", "kind=$REDACTED"),
        ("state=\"Pink Floyd/The Wall\"", "state=$REDACTED"),
        ("state=Pink(Floyd The Wall)", "state=$REDACTED"),
        ("state=(Pink Floyd)", "state=$REDACTED"),
        ("mode=a/b next words count=2", "mode=$REDACTED count=2"),
        ("state=/x/y Floyd The Wall id=3", "state=$REDACTED id=3"),
    ] {
        assert_eq!(redacted(input), expected, "{input}");
    }
}

#[test]
fn more_than_one_dot_is_a_version_only_under_the_version_key() {
    for input in ["count=192.168.1.5", "id=1.1.1.1", "elapsed_ms=0.1.331"] {
        let output = redacted(input);

        assert!(output.ends_with("=$REDACTED"), "{input}: {output}");
        assert!(!output.contains('.'), "{input}: {output}");
    }
    for input in ["count=1.5", "count=12", "id=0.25", "version=0.1.331"] {
        assert_eq!(redacted(input), input, "{input}");
    }
}

#[test]
fn a_debug_number_keeps_its_key_and_value() {
    for input in [
        "elapsed_ms=Some(1.5)",
        "elapsed_ms=Some(250)",
        "buffered_ms=None",
        "elapsed_ms=12.5ms",
        "elapsed_ms=3.5µs",
        "elapsed_ms=40us",
        "elapsed_ms=100ns",
        "elapsed_ms=2s",
        "elapsed_ms=Some(12.5ms)",
        "elapsed_ms=1.5e16",
        "elapsed_ms=1e-3",
        "elapsed_ms=2E+4",
        "elapsed_ms=-1.5e-3",
        "(elapsed_ms=Some(1.5)),",
    ] {
        assert_eq!(redacted(input), input, "{input}");
    }
}

#[test]
fn something_that_only_looks_like_a_debug_number_is_redacted() {
    for input in [
        "elapsed_ms=Some(Artist.flac)",
        "elapsed_ms=Some(1.5",
        "elapsed_ms=1.5.5ms",
        "elapsed_ms=1.e5",
        "elapsed_ms=1e",
        "elapsed_ms=12.5msx",
        "elapsed_ms=Some(1)(2)",
    ] {
        assert_eq!(redacted(input), "elapsed_ms=$REDACTED", "{input}");
    }
}

#[test]
fn a_redacted_key_takes_every_word_up_to_the_next_assignment() {
    for (input, expected) in [
        ("error=a b c", "error=$REDACTED"),
        ("error=a b count=3 more", "error=$REDACTED count=3 more"),
        ("error=a  b   count=3", "error=$REDACTED count=3"),
        ("prefix error=a b", "prefix error=$REDACTED"),
        ("error=a b\nnext line", "error=$REDACTED\nnext line"),
        ("path=x y; elapsed_ms=12", "path=$REDACTED elapsed_ms=12"),
        (
            "error=a b path=c d count=3",
            "error=$REDACTED path=$REDACTED count=3",
        ),
        ("error=a b ", "error=$REDACTED "),
    ] {
        assert_eq!(redacted(input), expected, "{input:?}");
    }
}

#[test]
fn a_secret_like_key_before_a_colon_takes_its_value_words_too() {
    for (input, expected) in [
        ("password: hunter2 and more", "password:$REDACTED"),
        ("password:hunter2 and more", "password:$REDACTED"),
        ("api_key: abc def count=3 tail", "api_key:$REDACTED"),
        ("status: all fine", "status: all fine"),
    ] {
        assert_eq!(redacted(input), expected, "{input:?}");
    }
}

#[test]
fn redacting_twice_changes_nothing() {
    // The log file's lines are redacted again when the report embeds them.
    for input in [
        "error=a b c count=3",
        "elapsed_ms=12;file=Artist",
        "password: hunter2",
        "state=Playing id=deadbeefcafe version=0.1.331-x",
    ] {
        let once = redacted(input);

        assert_eq!(redacted(&once), once, "{input}");
    }
}

#[test]
fn only_a_known_well_formed_key_ends_a_swallowed_value() {
    for (input, expected) in [
        ("error=boom data=abc== tail words", "error=$REDACTED"),
        ("error=x foo=bar more", "error=$REDACTED"),
        (
            "error=x a=b c=d count=3 tail",
            "error=$REDACTED count=3 tail",
        ),
        ("error=x Count=3 tail", "error=$REDACTED Count=3 tail"),
        ("error=x ==abc count=", "error=$REDACTED"),
        ("error=x 1count=3 y", "error=$REDACTED"),
        ("error=x (id=3) tail", "error=$REDACTED (id=3) tail"),
    ] {
        assert_eq!(redacted(input), expected, "{input}");
    }
}

#[test]
fn a_secret_like_colon_key_takes_the_rest_of_the_line_even_past_a_known_key() {
    assert_eq!(
        redacted("token: abc def count=3 tail words\nnext count=4"),
        "token:$REDACTED\nnext count=4"
    );
    assert_eq!(
        redacted("password: hunter2 error=x y"),
        "password:$REDACTED"
    );
}

#[test]
fn a_field_that_is_already_redacted_ends_the_previous_redaction_but_not_its_own() {
    assert_eq!(
        redacted("playlist=Roadtrip Mix user_name=$REDACTED track_id=42"),
        "playlist=$REDACTED user_name=$REDACTED track_id=42"
    );
    assert_eq!(
        redacted("user_name=$REDACTED leaked words id=3"),
        "user_name=$REDACTED id=3"
    );
}
