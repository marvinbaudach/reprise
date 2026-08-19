//! Centering the playing track after a model reload.
//!
//! Two things happen on this occasion, and only the first belongs here:
//! resolving *which* row, and then moving the viewport onto it. The move is
//! [`super::track_reveal::reveal_position`] — the same function the jump path
//! uses, at the motion this occasion needs — and the one arithmetic that puts
//! a row in the middle lives in [`write_centered`] below.
//!
//! ## One move, and what it costs to get one
//!
//! A reload hands the view a different list, and the allocation pass that
//! follows re-applies the view's *own* anchor over anything written before it.
//! This path used to cover that by snapping the row to the nearest viewport
//! edge first and refining to the middle afterwards — two moves, the second of
//! which is the hop SEARCH-16 rules out. Removing only the snap makes it worse
//! rather than better, and that is measured, not argued (2026-08-19): with the
//! snap gone, a result set that sat at its end takes the *new* list's end with
//! it, `6561.0 = upper - page`, written over a clean 2923.5 and written again
//! over the correction. The snap was not merely the visibility promise; it was
//! the anchoring.
//!
//! One move therefore needs two things, and neither is sufficient alone:
//!
//! - **The range, before the value.** [`ListGeometry::configure`] seeds
//!   `upper` from the row height this view already knows — persisted per
//!   density, measured on an earlier settled frame — and writes the value in
//!   the same call. Without it the target is clamped to the *old* list's
//!   range (714 for a 21-row result set) and the write is simply lost. This is
//!   what `reload_anchor_scroll::apply` does, and why a restore that lands on
//!   an anchor never had this hop.
//! - **A value GTK's anchor reproduces.** `scroll_to` aligns a row with the
//!   top of the viewport and nothing else, so the values a single move can
//!   hold are exactly the row edges. [`centered_anchor`] picks the edge
//!   nearest the arithmetic centre and hands GTK the row that explains it; the
//!   allocation pass then reproduces the value instead of correcting it. The
//!   price is at most half a row of off-centre — 0.5 px in the measured case.
//!
//! A prediction can still be wrong — a cold cache falls back to the CSS
//! token, which is only a lower bound. [`Centering::Predicted`] says so, and
//! the caller retries until the geometry proves itself; the correction is
//! then a fraction of a row rather than a trip through the top.
//!
//! ## Why this occasion does not claim `track_reveal_pending`
//!
//! The jump path sets that marker before it yields, so a reload landing in
//! the same main-loop turn sees the viewport is already spoken for and
//! anchors on the reveal's destination rather than on the frame it is passing
//! through (`track_list_reload::capture_reload_anchor`). The marker exists
//! for reloads to read. This *is* the reload, so claiming it would only make
//! the next capture anchor on a centering this same reload started — a reload
//! waiting on itself.

use std::rc::Rc;

use gtk4::prelude::{AdjustmentExt, ScrollableExt};

use super::track_reveal::RevealMotion;
use super::Shared;
use crate::ui::list_geometry::{ContentHeight, ListGeometry, RowHeightSource};
use crate::ui::list_geometry_layout::ListLayout;

/// Same budget as the jump path (`current_track_selection`): enough idle
/// rounds for a rebuilt list to allocate, and few enough that a list which
/// never settles reaches its visibility floor promptly.
const RESTORE_ATTEMPTS: u8 = 8;

pub(super) fn schedule(shared: &Rc<Shared>, track_id: Option<i64>, current_ids: &[i64]) {
    let anchor = track_id.map(|track_id| (track_id, 0.0));
    let Some(position) = super::reload_restore::prepaint_position(anchor, current_ids) else {
        return;
    };
    super::track_reveal::reveal_position(shared, position, RESTORE_ATTEMPTS, RevealMotion::Instant);
}

/// What one centering write could promise about the value it left behind.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Centering {
    /// Written against geometry the realized rows agree with. Final.
    Settled,
    /// Written against the remembered row height while the allocation is
    /// still catching up. Correct to within the error in that height, and
    /// worth re-running once the range proves itself.
    Predicted,
    /// Nothing was written: this view has no usable geometry at all yet.
    Unavailable,
    /// The list fits its viewport, so no row is off-centre to begin with.
    /// Load-bearing rather than an optimisation — the centering arithmetic
    /// answers `None` both for this and for "not allocated yet", and only the
    /// content height tells them apart. Without it a short result set burns
    /// every attempt and then snaps a row that was never out of view
    /// (`search_16_a_result_set_that_fits_still_centers_after_clear_all`).
    NothingToCenter,
}

/// Places row `position` in the middle of the viewport, seeding the
/// adjustment's geometry so the allocation pass that follows lands there too.
///
/// See the module documentation for why the seed and the anchor row are both
/// load-bearing.
pub(super) fn write_centered(shared: &Shared, position: u32, n_rows: u32) -> Centering {
    let Some(adjustment) = shared.column_view.vadjustment() else {
        return Centering::Unavailable;
    };
    let rows = n_rows as usize;
    let n_sections = shared.queue_sections.borrow().len();
    let geometry = ListGeometry::for_view(&shared.column_view);
    if fits_the_viewport(&geometry, shared, adjustment.page_size(), rows, n_sections) {
        return Centering::NothingToCenter;
    }
    // The remembered geometry, not one derived from the live `upper`:
    // deriving it is what made this path wait for the allocation it is trying
    // to steer.
    let Some(layout) = super::track_list_geometry::layout(shared, None, rows) else {
        return Centering::Unavailable;
    };
    let Some((anchor_row, target)) =
        centered_anchor(&layout, position, rows, adjustment.page_size())
    else {
        return Centering::Unavailable;
    };

    debug_assert!(
        !crate::ui::list_geometry_changed::in_changed_emission(),
        "centered scroll written from inside a changed emission"
    );
    if !crate::ui::scroll_probe::preseed_suppressed() {
        // Named before the call because `configure` writes the value along
        // with the range; an unnamed step in the trail reads as GTK's. The
        // range matters as much as the value: written against the *old*
        // list's `upper` — 714 for a 21-row result set — the centered value
        // is clamped to that list's end and the write is lost.
        crate::ui::scroll_probe::probe("centered.reveal.seed", &adjustment, target);
        geometry.configure(
            &adjustment,
            target,
            &shared.conn,
            &shared.list_geometry_cache,
            rows,
            n_sections,
        );
    }
    // Asked *after* the seed: the seeded range is the one the realized rows
    // have to agree with for this value to be final.
    let settled = geometry.is_settled(adjustment.upper(), rows, n_sections);
    shared
        .scroll_glide
        .jump_to(&adjustment, target, "centered.reveal.instant");
    // Hands GTK's own anchor to the row that explains the value just written,
    // in the same turn. `GtkListBase` re-applies its remembered anchor during
    // the allocation pass that follows a model swap; a pending `scroll_to`
    // replaces that anchor with this row, and because the value is already the
    // row's edge, reproducing it moves nothing. Firing it *before* the value —
    // which is what this path used to do — is the same call at the wrong
    // moment: there the allocation edge-snaps first and the centring is a
    // second, visible move.
    super::track_reveal::anchor_view_on(shared, anchor_row);
    if settled {
        Centering::Settled
    } else {
        Centering::Predicted
    }
}

/// The row GTK must anchor on, and the viewport value that anchoring produces
/// — the closest this widget can hold to `position` in the middle.
///
/// `GtkColumnView::scroll_to` is the only lever over the anchor, and it aligns
/// the requested row with the *top* of the viewport, unconditionally: asked to
/// reveal a row that is already centred on screen, it still moves the list so
/// the row starts at the top (measured 2026-08-19: from 2923.5 and from 2927.0
/// alike, `scroll_to(89)` produced 3026.0 = `89 × 34`). So the set of values a
/// single move can reach is exactly the set of row edges, and the honest
/// centring is the edge nearest the arithmetic centre — at most half a row away
/// from it, 0.5 px of a 239 px viewport in the measured case.
///
/// Choosing any other value costs a second, visible move: GTK re-applies its
/// own anchor during the allocation pass that follows a model swap, so a value
/// no anchor row explains is overwritten by one that does.
fn centered_anchor(
    layout: &ListLayout,
    position: u32,
    n_rows: usize,
    page_size: f64,
) -> Option<(u32, f64)> {
    if n_rows == 0 || position as usize >= n_rows || !page_size.is_finite() || page_size <= 0.0 {
        return None;
    }
    if layout.content_height(n_rows) <= page_size {
        return None;
    }
    let max_scroll = layout.max_scroll(n_rows, page_size);
    let centre = (layout.row_top(position) + layout.row_height().pixels() / 2.0 - page_size / 2.0)
        .clamp(0.0, max_scroll);
    let (row, below_its_top) = layout.row_at(centre);
    let next = row + 1;
    let anchor = if (next as usize) < n_rows && layout.row_top(next) - centre < below_its_top {
        next
    } else {
        row
    };
    // The same clamp GTK applies to the anchor it computes. Without it the
    // last rows of a list disagree by exactly the overshoot, and the
    // disagreement is a second move.
    Some((anchor, layout.row_top(anchor).clamp(0.0, max_scroll)))
}

/// Whether the whole list is on screen already.
///
/// An assumed row height can claim a list fits when it does not, so only a
/// measured one may answer yes.
fn fits_the_viewport(
    geometry: &ListGeometry,
    shared: &Shared,
    page_size: f64,
    n_rows: usize,
    n_sections: usize,
) -> bool {
    let (content, row_source, header_source) = geometry.content_height(
        &shared.conn,
        &shared.list_geometry_cache,
        n_rows,
        n_sections,
    );
    let measured = row_source == RowHeightSource::Measured
        && header_source.is_none_or(|source| source == RowHeightSource::Measured);
    matches!(content, ContentHeight::Known(pixels) if measured && pixels <= page_size)
}

#[cfg(test)]
mod tests {
    use super::centered_anchor;
    use crate::ui::list_geometry::RowHeight;
    use crate::ui::list_geometry_layout::ListLayout;

    fn height(pixels: f64) -> RowHeight {
        RowHeight::new(pixels).unwrap()
    }

    // The geometry the SEARCH-16 fixture actually runs on: 200 rows of 34 px
    // in a 239 px viewport, playing row 89. The arithmetic centre is 2923.5
    // and the edge next to it is 2924 — measured 2026-08-19 as the single
    // step the rebuilt restore takes.
    #[test]
    fn the_anchor_is_the_row_edge_nearest_the_arithmetic_centre() {
        let layout = ListLayout::rows_only(height(34.0));

        let (row, value) = centered_anchor(&layout, 89, 200, 239.0).unwrap();

        assert_eq!(row, 86, "row 86 starts at 2924, the edge next to 2923.5");
        assert_eq!(value, 2_924.0);
        assert!(
            (value - 2_923.5).abs() <= 34.0 / 2.0,
            "an anchored centering stays within half a row of the centre"
        );
    }

    // The value has to be one `scroll_to` can reproduce, or the allocation
    // pass overwrites it. Every answer is therefore a row top — and the right
    // one is the *nearest*, not the one the centre happens to sit in. Taking
    // the row it sits in would move the viewport a whole row further from the
    // middle than it needs to be.
    #[test]
    fn rounding_goes_to_the_nearer_edge_from_either_side() {
        let layout = ListLayout::rows_only(height(100.0));

        // Centre 495: inside row 4, but 5 px short of row 5's edge.
        assert_eq!(layout.row_at(495.0).0, 4);
        assert_eq!(centered_anchor(&layout, 5, 50, 110.0), Some((5, 500.0)));

        // Centre 505: inside row 5, 5 px past its own edge.
        assert_eq!(layout.row_at(505.0).0, 5);
        assert_eq!(centered_anchor(&layout, 5, 50, 90.0), Some((5, 500.0)));
    }

    // Section headers are part of where a row starts, so they are part of the
    // edge the anchor names. Reading the same layout the anchor restore reads
    // is what keeps our value and GTK's agreeing on a sectioned Queue; a
    // rows-only sum names a value the allocation pass then corrects.
    #[test]
    fn section_headers_move_the_edge_the_anchor_names() {
        let sectioned = ListLayout::sectioned(height(100.0), height(50.0), vec![0, 10]);
        let rows_only = ListLayout::rows_only(height(100.0));

        let (sectioned_row, sectioned_value) = centered_anchor(&sectioned, 20, 50, 300.0).unwrap();
        let (plain_row, plain_value) = centered_anchor(&rows_only, 20, 50, 300.0).unwrap();

        // Two headers precede row 20, so every edge below them sits 100 px
        // lower — the same row, a different place.
        assert_eq!((plain_row, plain_value), (19, 1_900.0));
        assert_eq!((sectioned_row, sectioned_value), (19, 2_000.0));
        assert_eq!(
            sectioned_value - plain_value,
            100.0,
            "the header band is the difference; ignoring it names a value GTK \
             does not reproduce"
        );
    }

    // The clamp is GTK's, not ours: an anchor row whose top lies past the end
    // of the scrollable range is scrolled to the end instead, and a value that
    // did not follow would be corrected in a second, visible move.
    #[test]
    fn the_last_rows_clamp_to_the_end_of_the_range() {
        let layout = ListLayout::rows_only(height(100.0));

        let (_, value) = centered_anchor(&layout, 49, 50, 300.0).unwrap();

        assert_eq!(value, layout.max_scroll(50, 300.0));
        assert_eq!(value, 4_700.0);
    }

    #[test]
    fn a_list_that_fits_or_a_row_it_does_not_have_names_no_anchor() {
        let layout = ListLayout::rows_only(height(100.0));

        assert_eq!(centered_anchor(&layout, 5, 10, 2_000.0), None);
        assert_eq!(centered_anchor(&layout, 10, 10, 300.0), None);
        assert_eq!(centered_anchor(&layout, 0, 0, 300.0), None);
        assert_eq!(centered_anchor(&layout, 5, 50, 0.0), None);
    }
}
