//! Shared vertical-centering math for uniform-row scrollables (`GtkListView` /
//! `GtkColumnView`).
//!
//! Both the track table (`track_list::current_track_selection`) and the Artists
//! master list (`library_views::artist_master`) center a selected row by writing
//! the vertical adjustment directly — a plain `scroll_to` only edge-snaps. The
//! geometry (uniform row height → row offset → viewport target) and the
//! vadjustment resolution are identical for both widgets; they differ only in
//! which `Scrollable` they act on and how they count their rows, so the shared
//! parts live here once and each caller passes in its own `n_rows`.

use gtk4::prelude::*;

/// Resolves the vertical adjustment and the value that vertically centers row
/// `position` in `scrollable`, given the list has `n_rows` (near-)uniform-height
/// rows. Returns `None` when the list has no usable geometry yet — not allocated
/// (`upper`/`page_size` unset), or it fits the viewport entirely — in which case
/// there is nothing to center.
pub(in crate::ui) fn centered_scroll_target(
    scrollable: &impl IsA<gtk4::Scrollable>,
    n_rows: u32,
    position: u32,
) -> Option<(gtk4::Adjustment, f64)> {
    let adjustment = scrollable.vadjustment()?;
    let value =
        centered_scroll_value(position, n_rows, adjustment.upper(), adjustment.page_size())?;
    Some((adjustment, value))
}

/// Adjustment value that vertically centers row `position`, assuming
/// (near-)uniform row heights, so a row's offset is derived from the
/// adjustment's total content height. Returns `None` when the list is
/// unallocated (`upper`/`page_size` unset) or fits entirely in the viewport.
pub(in crate::ui) fn centered_scroll_value(
    position: u32,
    n_rows: u32,
    upper: f64,
    page_size: f64,
) -> Option<f64> {
    if n_rows == 0 || upper <= 0.0 || page_size <= 0.0 || upper <= page_size {
        return None;
    }
    let row_height = upper / f64::from(n_rows);
    let target = (f64::from(position) + 0.5) * row_height - page_size / 2.0;
    Some(target.clamp(0.0, upper - page_size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_scroll_value_centers_a_mid_list_row() {
        // 100 rows x 10px = 1000px content, 200px viewport. Row 50's middle
        // sits at 505px; centering puts the viewport at 505 - 100 = 405.
        assert_eq!(centered_scroll_value(50, 100, 1000.0, 200.0), Some(405.0));
    }

    #[test]
    fn centered_scroll_value_clamps_at_both_list_edges() {
        assert_eq!(centered_scroll_value(0, 100, 1000.0, 200.0), Some(0.0));
        assert_eq!(centered_scroll_value(99, 100, 1000.0, 200.0), Some(800.0));
    }

    #[test]
    fn centered_scroll_value_skips_unallocated_or_short_lists() {
        // Not yet allocated: no geometry to work with.
        assert_eq!(centered_scroll_value(5, 100, 0.0, 0.0), None);
        // Whole list fits in the viewport: nothing to scroll.
        assert_eq!(centered_scroll_value(5, 10, 100.0, 200.0), None);
        // Empty model.
        assert_eq!(centered_scroll_value(0, 0, 1000.0, 200.0), None);
    }
}
