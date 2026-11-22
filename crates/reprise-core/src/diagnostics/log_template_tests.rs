//! The template model: parsing a format string, reducing it and matching a
//! rendered message against it.

use super::log_template::{LogTemplate, TemplateError};

fn template(format: &str) -> LogTemplate {
    LogTemplate::parse(format).expect("the fixture is a well-formed format string")
}

#[test]
fn a_template_without_holes_reduces_to_itself_and_matches_only_itself() {
    let plain = template("could not open the database");

    assert_eq!(plain.reduced(), "could not open the database");
    assert_eq!(plain.hole_count(), 0);
    assert!(plain.matches("could not open the database"));
    assert!(!plain.matches("could not open the database "));
    assert!(!plain.matches("we could not open the database"));
    assert!(!plain.matches(""));
}

#[test]
fn every_kind_of_hole_reduces_to_the_placeholder() {
    let holed = template("a {} b {name} c {:?} d {0:>5} e {x:#?}");

    assert_eq!(holed.reduced(), "a … b … c … d … e …");
    assert_eq!(holed.hole_count(), 5);
}

#[test]
fn doubled_braces_are_literal_braces() {
    let braces = template("{{not a hole}} and {}");

    assert_eq!(braces.reduced(), "{not a hole} and …");
    assert_eq!(braces.hole_count(), 1);
    assert!(braces.matches("{not a hole} and 7"));
    assert!(!braces.matches("{{not a hole}} and 7"));
}

#[test]
fn a_hole_matches_any_text_including_none() {
    let around = template("track {} loaded");

    assert!(around.matches("track 7 loaded"));
    assert!(around.matches("track Some Song Name loaded"));
    assert!(around.matches("track  loaded"));
    assert!(!around.matches("track loaded"));
}

#[test]
fn a_hole_may_be_empty_when_the_literals_touch() {
    let around = template("ab{}cd");

    assert!(around.matches("abcd"));
    assert!(around.matches("abXcd"));
    assert!(!around.matches("abc"));
}

#[test]
fn a_template_that_is_only_a_hole_matches_every_message() {
    let only = template("{message}");

    assert_eq!(only.reduced(), "…");
    assert_eq!(only.literal_chars(), 0);
    assert!(only.matches(""));
    assert!(only.matches("anything at all"));
}

#[test]
fn adjacent_holes_match_like_one_hole_and_reduce_to_one_placeholder_each() {
    let adjacent = template("x{}{}y");

    assert_eq!(adjacent.reduced(), "x……y");
    assert!(adjacent.matches("xy"));
    assert!(adjacent.matches("x12y"));
    assert!(!adjacent.matches("x12"));
}

#[test]
fn a_hole_whose_text_contains_the_next_literal_still_matches() {
    let template = template("a {} b {} c");

    assert!(template.matches("a b b b c"));
    assert!(template.matches("a x b b c"));
    assert!(template.matches("a b b c b c"));
    assert!(!template.matches("a b c"));
}

#[test]
fn the_literals_must_occur_in_the_template_order() {
    let ordered = template("{}one{}two{}");

    assert!(ordered.matches("..one..two.."));
    assert!(!ordered.matches("..two..one.."));
    assert!(!ordered.matches("one"));
}

#[test]
fn the_suffix_may_not_reuse_text_the_prefix_consumed() {
    let overlapping = template("ab{}ba");

    assert!(!overlapping.matches("aba"));
    assert!(overlapping.matches("abba"));
    assert!(overlapping.matches("ab-ba"));
}

#[test]
fn a_message_that_the_template_does_not_cover_does_not_match() {
    let anchored = template("loaded {} tracks");

    assert!(!anchored.matches("loaded 4 tracks, then crashed"));
    assert!(!anchored.matches("then loaded 4 tracks"));
    assert!(!anchored.matches("loaded 4"));
    assert!(!anchored.matches("completely different"));
}

#[test]
fn matching_works_on_multibyte_text() {
    let accented = template("Café: {} … done");

    assert!(accented.matches("Café: 東京 … done"));
    assert!(accented.matches("Café:  … done"));
    assert!(!accented.matches("Cafe: x … done"));
}

#[test]
fn a_very_long_message_matches_without_recursion() {
    let long = "x".repeat(200_000);
    let rendered = format!("a {long} b {long} c");

    assert!(template("a {} b {} c").matches(&rendered));
    assert!(!template("a {} b {} d").matches(&rendered));
}

#[test]
fn literal_chars_counts_characters_not_bytes_and_not_holes() {
    assert_eq!(template("ab{}cd").literal_chars(), 4);
    assert_eq!(template("ö{}ü").literal_chars(), 2);
    assert_eq!(template("{{}}").literal_chars(), 2);
}

#[test]
fn the_canonical_form_round_trips_and_normalises_holes() {
    let original = template("use {{braces}} and {name} then {:?}");

    assert_eq!(original.canonical(), "use {{braces}} and {} then {}");
    assert_eq!(template(&original.canonical()), original);
}

#[test]
fn malformed_format_strings_are_refused() {
    assert_eq!(
        LogTemplate::parse("open {"),
        Err(TemplateError::UnclosedHole)
    );
    assert_eq!(
        LogTemplate::parse("open {name"),
        Err(TemplateError::UnclosedHole)
    );
    assert_eq!(
        LogTemplate::parse("nested {a{b}}"),
        Err(TemplateError::UnclosedHole)
    );
    assert_eq!(
        LogTemplate::parse("close }"),
        Err(TemplateError::StrayClose)
    );
    assert_eq!(LogTemplate::parse("}"), Err(TemplateError::StrayClose));
    assert!(LogTemplate::parse("").is_ok());
}

#[test]
fn the_empty_template_matches_only_the_empty_message() {
    let empty = template("");

    assert!(empty.matches(""));
    assert!(!empty.matches("x"));
    assert_eq!(empty.reduced(), "");
}
