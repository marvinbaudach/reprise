//! The shape of the two field lists, and what the predicate does with them.

use super::field_policy::{KEEP_FIELDS, REDACT_FIELDS};
use super::is_safe_structured_field;

/// Keys the maintainers decided stay redacted whatever a site carries.
const NEVER_KEPT: [&str; 25] = [
    "error",
    "message",
    "detail",
    "reason",
    "source",
    "path",
    "dir",
    "file",
    "root",
    "url",
    "uri",
    "name",
    "title",
    "artist",
    "album",
    "playlist",
    "playlist_name",
    "device_id",
    "device_name",
    "value",
    "command",
    "from",
    "to",
    "key",
    "item",
];

#[test]
fn both_lists_are_sorted_without_duplicates() {
    for (name, list) in [
        ("KEEP_FIELDS", KEEP_FIELDS),
        ("REDACT_FIELDS", REDACT_FIELDS),
    ] {
        for pair in list.windows(2) {
            assert!(
                pair[0] < pair[1],
                "{name} must be strictly sorted, but `{}` comes before `{}`",
                pair[0],
                pair[1]
            );
        }
    }
}

#[test]
fn no_key_is_in_both_lists() {
    let both: Vec<&&str> = KEEP_FIELDS
        .iter()
        .filter(|key| REDACT_FIELDS.binary_search(key).is_ok())
        .collect();

    assert!(both.is_empty(), "keys in both lists: {both:?}");
}

#[test]
fn every_listed_key_is_lowercase_ascii() {
    // The free-text pass lowercases a key before the lookup, and the typed
    // pass compares it as written, so a key with an upper-case letter would
    // be safe in one pass and redacted in the other.
    for key in KEEP_FIELDS.iter().chain(REDACT_FIELDS) {
        assert!(
            !key.is_empty()
                && key.chars().all(|ch| ch.is_ascii_lowercase()
                    || ch.is_ascii_digit()
                    || ch == '_'
                    || ch == '.'),
            "`{key}` must be lowercase ASCII, digits, `_` or `.`"
        );
    }
}

#[test]
fn the_maintainers_named_identifying_keys_are_never_kept() {
    for key in NEVER_KEPT {
        assert!(!is_safe_structured_field(key), "`{key}` must stay redacted");
        assert!(
            KEEP_FIELDS.binary_search(&key).is_err(),
            "`{key}` must not be in KEEP_FIELDS"
        );
    }
}

#[test]
fn unknown_and_near_miss_keys_are_not_safe() {
    for key in [
        "",
        "unknown_field",
        "foo_id",
        "elapsed",
        "Track_Id",
        "track_id ",
        "is_ready",
    ] {
        assert!(!is_safe_structured_field(key), "`{key}` must not be safe");
    }
}

#[test]
fn durations_counts_states_and_internal_ids_are_safe() {
    for key in [
        "elapsed_ms",
        "count",
        "state",
        "track_id",
        "episode_id",
        "playlist_id",
        "position_ms",
        "queue_len",
        "musicbrainz_error",
        "version",
    ] {
        assert!(is_safe_structured_field(key), "`{key}` should be safe");
    }
}

#[test]
fn the_predicate_is_a_lookup_in_the_keep_list() {
    for key in KEEP_FIELDS {
        assert!(
            is_safe_structured_field(key),
            "`{key}` is listed but unsafe"
        );
    }
    for key in REDACT_FIELDS {
        assert!(
            !is_safe_structured_field(key),
            "`{key}` is redacted but safe"
        );
    }
}
