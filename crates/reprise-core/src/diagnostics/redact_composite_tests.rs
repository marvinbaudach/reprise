//! Composite `Debug` values under a kept key: `Some(Track(5))`, `[1, 2, 3]`,
//! `[NowPlaying, PlayNext(0)]`. Only plain parts survive, and a value that is
//! rejected owns the words after it exactly as any other rejected value.

use super::{redact_log_message, RedactionContext};

fn redacted(message: &str) -> String {
    redact_log_message(message, &RedactionContext::default())
}

#[test]
fn an_option_of_a_plain_value_keeps_its_key_and_value() {
    for input in [
        "current_up_next=Some(Track(5))",
        "current_up_next=Some(Episode(12))",
        "current_up_next=None",
        "deleted=Some(7)",
        "state=Some(Playing)",
        "state=Some(true)",
        "state=Some(Some(Some(1)))",
        "state=Some(Track(-5))",
        "(current_up_next=Some(Track(5))),",
        "runtime_current_up_next=Some(Track(5)) count=2",
        "current_up_next=Some(Track(5)); count=2",
    ] {
        assert_eq!(redacted(input), input, "{input}");
    }
}

#[test]
fn a_list_of_plain_values_keeps_its_key_and_value() {
    for input in [
        "episode_ids=[]",
        "episode_ids=[1]",
        "episode_ids=[1, 2, 3]",
        "rows=[NowPlaying, PlayNext(0), UpNext(1)]",
        "rows=[Some(1), None]",
        "(episode_ids=[1, 2, 3]),",
        "episode_ids=[1, 2, 3] count=2",
        "episode_ids=[1, 2, 3]; count=2",
        "before rows=[NowPlaying, UpNext(0)] after",
    ] {
        assert_eq!(redacted(input), input, "{input}");
    }
}

#[test]
fn a_list_at_the_element_bound_survives_and_one_more_does_not() {
    let list = |count: usize| {
        let elements = (0..count)
            .map(|index| index.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        format!("episode_ids=[{elements}] count=2")
    };

    assert_eq!(redacted(&list(64)), list(64));
    assert_eq!(redacted(&list(65)), "episode_ids=$REDACTED");
    assert_eq!(redacted(&list(100)), "episode_ids=$REDACTED");
}

#[test]
fn a_composite_value_with_anything_but_plain_parts_is_redacted_with_its_words() {
    for (input, expected) in [
        (
            "current_up_next=Some(\"Pink Floyd\") count=2",
            "current_up_next=$REDACTED",
        ),
        (
            "current_up_next=Some(/home/x) count=2",
            "current_up_next=$REDACTED",
        ),
        ("episode_ids=[1, /home/x] count=2", "episode_ids=$REDACTED"),
        ("episode_ids=[Artist Name] count=2", "episode_ids=$REDACTED"),
        (
            "current_up_next=Some(Track(deadbeefcafe)) count=2",
            "current_up_next=$REDACTED",
        ),
        ("episode_ids=[1, 2 count=2", "episode_ids=$REDACTED"),
        ("episode_ids=[1, 2 tail words only", "episode_ids=$REDACTED"),
        ("state=Some(Some(Some(Some(1)))) count=2", "state=$REDACTED"),
        ("episode_ids=[1, 2]x count=2", "episode_ids=$REDACTED"),
        ("episode_ids=[[1], [2]] count=2", "episode_ids=$REDACTED"),
        ("episode_ids=[1, ] count=2", "episode_ids=$REDACTED"),
        ("episode_ids=[1.5, 2] count=2", "episode_ids=$REDACTED"),
        ("episode_ids=[a.flac, b] count=2", "episode_ids=$REDACTED"),
        ("episode_ids=[a:b] count=2", "episode_ids=$REDACTED"),
        ("state=Some(Track(1.5)) count=2", "state=$REDACTED"),
        ("state=Some(Track()) count=2", "state=$REDACTED"),
        ("state=Some() count=2", "state=$REDACTED"),
    ] {
        assert_eq!(redacted(input), expected, "{input}");
    }
}

#[test]
fn a_list_on_a_key_that_is_not_kept_is_redacted() {
    assert_eq!(redacted("volume=[1, 2, 3] count=2"), "volume=$REDACTED");
    assert_eq!(redacted("error=[1, 2, 3] count=2"), "error=$REDACTED");
}

#[test]
fn a_struct_like_value_is_redacted_with_its_fields() {
    for (input, expected) in [
        ("rows=QueueRow { index: 1 } count=2", "rows=$REDACTED"),
        ("rows=[QueueRow { index: 1 }] count=2", "rows=$REDACTED"),
        (
            "current_up_next=Some(Track { id: 5 }) count=2",
            "current_up_next=$REDACTED",
        ),
    ] {
        assert_eq!(redacted(input), expected, "{input}");
    }
}

#[test]
fn a_list_split_by_a_line_break_or_a_wide_gap_is_not_one_value() {
    // The value is redacted to the end of its line. After a line break the
    // next line starts afresh, so the rest of the list shows there.
    for (input, expected) in [
        (
            "episode_ids=[1,\n2] count=2",
            "episode_ids=$REDACTED\n2] count=2",
        ),
        ("episode_ids=[1,  2] count=2", "episode_ids=$REDACTED"),
        ("episode_ids=[1,\t2] count=2", "episode_ids=$REDACTED"),
    ] {
        assert_eq!(redacted(input), expected, "{input:?}");
    }
}

#[test]
fn redacting_a_composite_value_twice_changes_nothing() {
    for input in [
        "current_up_next=Some(Track(5)) rows=[NowPlaying, UpNext(0)] episode_ids=[]",
        "episode_ids=[1, /home/x] count=2",
        "episode_ids=[1, 2",
    ] {
        let once = redacted(input);

        assert_eq!(redacted(&once), once, "{input}");
    }
}
