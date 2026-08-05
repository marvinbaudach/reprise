use gtk4::prelude::*;

use super::super::review_header::ReviewHeader;
use super::{build_row, narrow_prefixed, strike_range, value_label, ReviewLayout, ValueKind};

/// A window this wide is an ordinary maximised desktop window. Everything the
/// review row promises has to be readable inside it.
const DESKTOP_WIDTH: i32 = 1760;

/// A label that ellipsizes still asks for its whole text unless something caps
/// its natural width. Bound into a horizontal size group, that request becomes
/// the column's width for every row — and the columns to its right leave the
/// page, silently, because the list refuses to scroll sideways.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_3b_a_long_value_does_not_widen_its_column_without_bound() {
    gtk4::init().unwrap();
    let label = value_label(false, super::VALUE_MAX_CHARS);
    label.set_text(&"unreasonably descriptive track title ".repeat(8));

    let (_, natural, _, _) = label.measure(gtk4::Orientation::Horizontal, -1);

    assert!(
        natural < DESKTOP_WIDTH / 3,
        "one value wants {natural}px; three of those plus track, field and \
         source cannot fit a {DESKTOP_WIDTH}px window"
    );
}

/// The regression this pins: with long values in the rows, the shared header
/// grew past the window and Current, Proposed and Source were rendered outside
/// it. The user saw that a year would change but never to what.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_3b_every_column_still_fits_a_desktop_window_with_long_values() {
    gtk4::init().unwrap();
    let header = ReviewHeader::new();
    let widgets = build_row(&header.groups);
    let long = "unreasonably descriptive track title that never ends".repeat(3);
    widgets.track.set_text(&long);
    widgets.field.set_text(&long);
    widgets.current.set_text(&long);
    widgets.proposed.set_text(&long);
    widgets.source.set_text(&long);

    let (_, header_natural, _, _) = header.root.measure(gtk4::Orientation::Horizontal, -1);
    let (_, row_natural, _, _) = widgets.root.measure(gtk4::Orientation::Horizontal, -1);

    assert!(
        header_natural <= DESKTOP_WIDTH,
        "the shared header wants {header_natural}px in a {DESKTOP_WIDTH}px window"
    );
    assert!(
        row_natural <= DESKTOP_WIDTH,
        "a row wants {row_natural}px in a {DESKTOP_WIDTH}px window"
    );
}

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
