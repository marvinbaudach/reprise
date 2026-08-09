//! STYLE-9 for the secondary tables (releases, radio, concerts).
//!
//! A `GtkColumnViewColumn` left at its default `fixed-width = -1` "grows to
//! fit its contents" — and the only contents it can see are the cells it has
//! realised *right now*. `ColumnView` recycles row widgets while scrolling,
//! so each batch of rows scrolled into view re-measures the column and the
//! whole table shifts sideways under the pointer. The music library has never
//! done this because `track_list::column_layout` pins every track column; see
//! `examples/column_width_scroll_repro.rs` for the measured difference
//! (191 px of drift unpinned, 0 px pinned).
//!
//! The rule is therefore: **every** column is pinned via [`pin`], and exactly
//! one — the table's main text column — additionally expands via
//! [`pin_filler`] to absorb the leftover width. Pinning does not take the
//! width away from the user: these columns stay `resizable`, and a header
//! drag simply writes a new fixed width.
//!
//! `instability` states that contract as a check the per-table `style_9_…`
//! tests run against their real `append_columns`.

// Only the test-only helpers below traverse widgets and downcast; pinning
// itself needs no trait imports.
#[cfg(test)]
use gtk4::prelude::*;

/// A compact square release-artwork cell.
pub(in crate::ui) const COVER: i32 = 40;
/// A date cell — mirrors the library's Added column.
pub(in crate::ui) const DATE: i32 = 160;
/// Lower bound for the filler column — mirrors the library's Title column,
/// low enough that a narrow window can still shrink the table.
pub(in crate::ui) const TITLE_MIN: i32 = 120;
/// A person or show name — mirrors the library's Artist column.
pub(in crate::ui) const NAME: i32 = 260;
/// A secondary label (genre, city, venue).
pub(in crate::ui) const LABEL: i32 = 180;
/// A one-word label (release type, country).
pub(in crate::ui) const SHORT_LABEL: i32 = 120;
/// A numeric readout (duration, bitrate, distance) — mirrors Duration.
pub(in crate::ui) const NUMERIC: i32 = 100;
/// A status pill.
pub(in crate::ui) const PILL: i32 = 140;
/// A cell holding a text button (Buy, Tickets).
pub(in crate::ui) const ACTION: i32 = 120;
/// A cell holding an icon-only button or state glyph.
pub(in crate::ui) const ICON_ACTION: i32 = 56;

/// Pins `column` to `width`, so scrolling can no longer re-measure it.
pub(in crate::ui) fn pin(column: &gtk4::ColumnViewColumn, width: i32) {
    column.set_expand(false);
    column.set_fixed_width(width);
}

/// Pins `column` to `min_width` *and* lets it absorb the table's leftover
/// width. Exactly one column per table may be the filler — two of them split
/// the leftover space between each other, which is stable, but leaves the
/// table without a single column that owns the slack.
pub(in crate::ui) fn pin_filler(column: &gtk4::ColumnViewColumn, min_width: i32) {
    column.set_fixed_width(min_width);
    column.set_expand(true);
}

/// One column's width decision, so a table's generic `text_column` helper can
/// carry it as a single argument instead of a `(width, is_filler)` pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) struct Sizing {
    width: i32,
    filler: bool,
}

impl Sizing {
    /// A column at a fixed width.
    pub(in crate::ui) const fn pinned(width: i32) -> Self {
        Self {
            width,
            filler: false,
        }
    }

    /// The one column per table that absorbs the leftover width, never below
    /// `min_width`.
    pub(in crate::ui) const fn filler(min_width: i32) -> Self {
        Self {
            width: min_width,
            filler: true,
        }
    }

    pub(in crate::ui) fn apply(self, column: &gtk4::ColumnViewColumn) {
        if self.filler {
            pin_filler(column, self.width);
        } else {
            pin(column, self.width);
        }
    }
}

/// Ways `view` can still shift while scrolling, as human-readable lines.
/// Empty means the table honours the contract above.
#[cfg(test)]
pub(in crate::ui) fn instability(view: &gtk4::ColumnView) -> Vec<String> {
    let columns = view.columns();
    let mut problems = Vec::new();
    let mut fillers = Vec::new();
    for index in 0..columns.n_items() {
        let Some(column) = columns.item(index).and_downcast::<gtk4::ColumnViewColumn>() else {
            continue;
        };
        let name = column
            .title()
            .map_or_else(|| format!("column {index}"), |title| title.to_string());
        if column.fixed_width() <= 0 {
            problems.push(format!(
                "{name}: unpinned (fixed_width={}), so its width follows whatever rows are on screen",
                column.fixed_width()
            ));
        }
        if column.expands() {
            fillers.push(name);
        }
    }
    if fillers.len() != 1 {
        problems.push(format!(
            "expected exactly one filler column, found {}: {fillers:?}",
            fillers.len()
        ));
    }
    problems
}

/// Test-only: the realised width of every column, read off the header. Each
/// `GtkColumnViewTitle` is allocated exactly its column's width, including
/// the columns that carry no title text.
#[cfg(test)]
pub(in crate::ui) fn realised_widths(view: &gtk4::ColumnView) -> Vec<i32> {
    fn walk(widget: &gtk4::Widget, out: &mut Vec<i32>) {
        if widget.type_().name() == "GtkColumnViewTitle" {
            out.push(widget.width());
            return;
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            walk(&current, out);
            child = current.next_sibling();
        }
    }
    let mut out = Vec::new();
    walk(view.upcast_ref::<gtk4::Widget>(), &mut out);
    out
}

/// Test-only: proves STYLE-9's "measured, not asserted" half for a table — the
/// columns must not move when the rows on screen change, which is what
/// scrolling does to a recycled `ColumnView`. `swap_in_longer_rows` replaces
/// the model's rows with markedly wider content.
///
/// Takes `table` unrealised; it presents it in its own window so the caller
/// only has to describe its own table.
#[cfg(test)]
pub(in crate::ui) fn assert_stable_across_row_change(
    table: &gtk4::ColumnView,
    swap_in_longer_rows: impl FnOnce(),
) {
    let problems = instability(table);
    assert!(
        problems.is_empty(),
        "unstable column contract: {problems:?}"
    );

    let window = gtk4::Window::new();
    window.set_default_size(1200, 400);
    window.set_child(Some(table));
    window.present();
    crate::ui::source_context_surface::settle_layout();

    let before = realised_widths(table);
    assert!(
        before.iter().all(|width| *width > 0),
        "no realised columns to measure: {before:?}"
    );

    swap_in_longer_rows();
    crate::ui::source_context_surface::settle_layout();

    let after = realised_widths(table);
    assert_eq!(
        before, after,
        "columns moved when the rows on screen changed — the table shifts while scrolling"
    );
}
