use super::{narrow_prefixed, strike_range, ReviewLayout, ValueKind};

#[test]
fn doc_9b_rows_carry_no_caption_labels() {
    let source = include_str!("review_row.rs");

    assert!(!source.contains("value_widgets("));
}

/// Wide rows are named by the shared header above them. Narrow rows have no
/// header — it is hidden below the breakpoint — so the value has to say which
/// column it came from, or the user reads three bare strings in a stack.
#[test]
fn doc_3b_narrow_rows_name_their_values_and_wide_rows_do_not() {
    let wide = narrow_prefixed(ReviewLayout::Wide, ValueKind::Current, "The beatles");
    let narrow = narrow_prefixed(ReviewLayout::Narrow, ValueKind::Current, "The beatles");

    assert_eq!(wide, "The beatles", "the header already names this column");
    assert!(narrow.contains("The beatles"), "the value must survive");
    assert!(
        narrow.len() > wide.len(),
        "the narrow layout adds a prefix: {narrow}"
    );
}

/// Each of the three values gets its own word — a stack of identically
/// prefixed lines would be no better than no prefix at all.
#[test]
fn doc_3b_each_narrow_value_carries_a_distinct_prefix() {
    let current = narrow_prefixed(ReviewLayout::Narrow, ValueKind::Current, "x");
    let proposed = narrow_prefixed(ReviewLayout::Narrow, ValueKind::Proposed, "x");
    let source = narrow_prefixed(ReviewLayout::Narrow, ValueKind::Source, "x");

    assert_ne!(current, proposed);
    assert_ne!(proposed, source);
    assert_ne!(current, source);
}

/// The prefix is a label, not a superseded value. Striking it through would
/// say "Now:" is what changed.
#[test]
fn doc_3b_the_strikethrough_covers_the_value_and_not_its_prefix() {
    let value = "The beatles";
    let rendered = narrow_prefixed(ReviewLayout::Narrow, ValueKind::Current, value);
    let (start, end) = strike_range(&rendered, value);

    assert!(
        start > 0,
        "a prefix precedes the value in the narrow layout"
    );
    assert_eq!(
        &rendered[start as usize..end as usize],
        value,
        "the struck range must be exactly the old value"
    );
}

/// In the wide layout the rendered text *is* the value, so the range covers
/// all of it — the same call site works for both layouts.
#[test]
fn doc_3b_the_strikethrough_covers_a_wide_value_whole() {
    let value = "The beatles";
    let rendered = narrow_prefixed(ReviewLayout::Wide, ValueKind::Current, value);

    assert_eq!(strike_range(&rendered, value), (0, value.len() as u32));
}
