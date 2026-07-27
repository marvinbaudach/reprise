//! TAG-1: pure-logic helpers that let `track_list_reload::reload` become
//! navigation-neutral — selection and scroll survive a `reload()` model
//! swap for every caller (sort clicks, rating edits, DnD, tag-editor save,
//! watcher reconcile, …), keyed on stable track ids rather than positions
//! (which shift under sort/filter) or an absolute scroll pixel value (which
//! points at the wrong row once row order changes underneath it).
//!
//! Deliberately GTK-free so the restore math is unit-testable without a
//! display. The GTK-touching half — reading the live selection/scroll
//! adjustment before the swap, and applying the computed positions/scroll
//! value after it — lives in `track_list_reload.rs`, which is also the only
//! place that knows *why* a single `track_at` lookup is used for the anchor
//! row instead of scanning every row: `TrackListModel` lazily windows its
//! rows from SQL (see that module's doc comment), so building a full id list
//! here via the widget model would force-load an entire library on every
//! reload. The full new-model id list this module's functions are matched
//! against instead comes from `Shared::current_view_ids()` — a single
//! dedicated query, not a per-row model walk.
//!
//! ## Deviation from the taskplan's sketched interface
//!
//! The taskplan's `capture(...) -> ReloadAnchor` was sketched against
//! `Shared`/GTK types. Kept here as a pure constructor over pre-extracted
//! primitives instead (`selected_ids` + an already-resolved `(track_id,
//! offset)` anchor tuple) so this module stays GTK-free; the extraction
//! itself (selection bitset walk, viewport-top row lookup) is a thin
//! wrapper in `track_list_reload.rs`.

use std::collections::HashSet;

/// Snapshot needed to restore selection + scroll position across a
/// `reload()` model swap. `anchor` is the track id currently anchoring the
/// viewport's top edge, paired with its pixel offset *into* that row (i.e.
/// how far the viewport's top edge has scrolled past the row's own top edge)
/// — never a raw scroll value, which means nothing once the row order
/// changes.
#[derive(Clone, Debug, Default, PartialEq)]
pub(in crate::ui) struct ReloadAnchor {
    pub selected_ids: Vec<i64>,
    pub anchor: Option<(i64, f64)>,
}

/// Pure constructor: bundles already-extracted primitives into a
/// `ReloadAnchor`. See the module doc for why this doesn't take `Shared`/GTK
/// types directly.
pub(in crate::ui) fn capture(selected_ids: Vec<i64>, anchor: Option<(i64, f64)>) -> ReloadAnchor {
    ReloadAnchor {
        selected_ids,
        anchor,
    }
}

/// Whether restoring this anchor could change anything. An untouched list —
/// nothing selected, sitting at the very top — has nothing to restore: the
/// rebuilt list is already unselected and already at the top, so the capture
/// side records no anchor at all for it (see `track_list_reload::
/// capture_reload_anchor`).
///
/// Callers use this to skip the restore path entirely, which matters because
/// resolving positions needs the view's full id list
/// (`Shared::current_view_ids()`) — a sorted full-table query. Watcher
/// reconciles and scan progress fire `reload()` in bursts on lists nobody is
/// looking at; without this guard each one would pay for that query only to
/// restore a no-op.
///
/// Note this is deliberately *not* "the offset happens to be 0.0": an anchor
/// sitting flush against row 50's top edge also has offset 0.0, and dropping
/// its restore would snap the view back to the top.
pub(in crate::ui) fn is_noop(anchor: &ReloadAnchor) -> bool {
    anchor.selected_ids.is_empty() && anchor.anchor.is_none()
}

/// Maps `ids` onto their positions within `current`, in `current`'s order.
/// Ids no longer present — deleted tracks, or ones a changed filter/playlist
/// membership dropped — are silently omitted rather than reported as an
/// error; a gone id is simply no longer part of the selection (TAG-1: "a
/// deliberate reset is explicit, never a side effect").
pub(in crate::ui) fn positions_for_ids(ids: &[i64], current: &[i64]) -> Vec<u32> {
    let wanted: HashSet<i64> = ids.iter().copied().collect();
    current
        .iter()
        .enumerate()
        .filter(|(_, id)| wanted.contains(id))
        .filter_map(|(position, _)| u32::try_from(position).ok())
        .collect()
}

/// Resolves the stable scroll anchor to its new row before GTK paints the
/// rebuilt model. The widget layer uses this for `ColumnView::scroll_to`
/// immediately after the query swap, preventing the full-range
/// `items_changed` signal from exposing its transient position-zero state.
/// The later pixel calculation still restores the precise within-row offset.
pub(in crate::ui) fn prepaint_position(
    anchor: Option<(i64, f64)>,
    current_ids: &[i64],
) -> Option<u32> {
    let (anchor_id, _) = anchor?;
    let position = current_ids.iter().position(|&id| id == anchor_id)?;
    u32::try_from(position).ok()
}

/// Computes the scroll offset that keeps the anchor row at the same
/// distance from the viewport top it had when captured, resolved against the
/// anchor track's *new* position in `current_ids`. Returns `None` when there
/// is no anchor, or the anchored track no longer exists (e.g. it was
/// deleted) — callers leave the scroll position untouched in that case
/// rather than guessing.
pub(in crate::ui) fn scroll_target(
    anchor: Option<(i64, f64)>,
    current_ids: &[i64],
    row_height: f64,
    viewport_height: f64,
) -> Option<f64> {
    let (anchor_id, offset) = anchor?;
    let position = current_ids.iter().position(|&id| id == anchor_id)?;
    let row_top = position as f64 * row_height;
    let target = row_top + offset;
    let content_height = current_ids.len() as f64 * row_height;
    let upper_bound = (content_height - viewport_height).max(0.0);
    Some(target.clamp(0.0, upper_bound))
}

/// Computes the scroll offset that vertically centers `track_id` after a
/// filter change. Returns `None` when no playing track exists, the track is
/// outside the new view, or the whole list fits in the viewport.
pub(in crate::ui) fn centered_track_scroll_target(
    track_id: Option<i64>,
    current_ids: &[i64],
    row_height: f64,
    viewport_height: f64,
) -> Option<f64> {
    let track_id = track_id?;
    let position = current_ids.iter().position(|&id| id == track_id)?;
    let position = u32::try_from(position).ok()?;
    let n_rows = u32::try_from(current_ids.len()).ok()?;
    let content_height = current_ids.len() as f64 * row_height;
    crate::ui::scroll_center::centered_scroll_value(
        position,
        n_rows,
        content_height,
        viewport_height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acc_6_dynamic_updates_preserve_logical_focus() {
        let surviving = positions_for_ids(&[7, 3, 11], &[11, 7, 19]);
        assert_eq!(surviving, [0, 1]);

        let anchor = ReloadAnchor {
            selected_ids: vec![7, 3, 11],
            anchor: Some((7, 0.5)),
        };
        assert!(!is_noop(&anchor));
    }

    #[test]
    fn tag_1_positions_for_ids_maps_surviving_ids_only() {
        // 11 no longer exists in the current view; 9 and 7 survived (in new
        // positions after a resort).
        assert_eq!(positions_for_ids(&[7, 9, 11], &[9, 42, 7]), vec![0, 2]);
    }

    #[test]
    fn tag_1_scroll_target_follows_anchor_row_after_resort() {
        // Anchor was captured at row 2 (5px scrolled into it, 20px rows).
        // A resort moved that same track id (100) to position 7 — the
        // restored scroll must follow the row's NEW position.
        let anchor = Some((100_i64, 5.0));
        let current_ids = vec![10, 20, 30, 40, 50, 60, 70, 100, 80, 90];
        assert_eq!(scroll_target(anchor, &current_ids, 20.0, 50.0), Some(145.0));
    }

    #[test]
    fn fil_9_filter_change_centers_playing_track_in_new_results() {
        let current_ids = (1..=100).collect::<Vec<_>>();

        assert_eq!(
            centered_track_scroll_target(Some(51), &current_ids, 20.0, 200.0),
            Some(910.0)
        );
    }

    #[test]
    fn tag_1_prepaint_target_resolves_the_stable_anchor_before_offset_restore() {
        let anchor = Some((100_i64, 5.0));
        let current_ids = vec![10, 20, 30, 40, 50, 60, 70, 100, 80, 90];

        assert_eq!(prepaint_position(anchor, &current_ids), Some(7));
        assert_eq!(prepaint_position(Some((999, 0.0)), &current_ids), None);
        assert_eq!(prepaint_position(None, &current_ids), None);
    }

    #[test]
    fn tag_1_scroll_target_none_when_anchor_gone() {
        let anchor = Some((999_i64, 5.0));
        let current_ids = vec![10, 20, 30];
        assert_eq!(scroll_target(anchor, &current_ids, 20.0, 50.0), None);
    }

    #[test]
    fn tag_1_deleted_ids_drop_silently() {
        // Both the selection and the scroll anchor pointed at tracks that
        // vanished (10 and 30 deleted) — both restores degrade silently
        // (empty selection positions, no scroll change) rather than panic
        // or select something unrelated.
        let ids = vec![10, 20, 30];
        let current = vec![20];
        assert_eq!(positions_for_ids(&ids, &current), vec![0]);

        let anchor = Some((10_i64, 5.0));
        assert_eq!(scroll_target(anchor, &current, 20.0, 50.0), None);
    }

    #[test]
    fn scroll_target_returns_none_without_an_anchor() {
        assert_eq!(scroll_target(None, &[1, 2, 3], 20.0, 50.0), None);
    }

    #[test]
    fn scroll_target_clamps_to_content_height() {
        // Anchor at the very last row with a large offset would overshoot
        // past the end of the content; clamp to the last valid scroll
        // position instead of requesting an out-of-range value.
        let anchor = Some((3_i64, 500.0));
        let current_ids = vec![1, 2, 3];
        // content = 3 * 20 = 60, viewport = 50 -> upper bound = 10
        assert_eq!(scroll_target(anchor, &current_ids, 20.0, 50.0), Some(10.0));
    }

    #[test]
    fn scroll_target_zero_when_content_fits_viewport() {
        let anchor = Some((2_i64, 5.0));
        let current_ids = vec![1, 2, 3];
        // content = 60, viewport = 200 -> fits entirely, upper bound clamps
        // to 0 regardless of the row's own offset.
        assert_eq!(scroll_target(anchor, &current_ids, 20.0, 200.0), Some(0.0));
    }

    #[test]
    fn positions_for_ids_empty_saved_and_empty_current_are_both_fine() {
        assert_eq!(positions_for_ids(&[], &[1, 2]), Vec::<u32>::new());
        assert_eq!(positions_for_ids(&[1], &[]), Vec::<u32>::new());
    }

    #[test]
    fn is_noop_only_for_an_untouched_list() {
        // Nothing selected, no anchor recorded (capture side saw scroll 0):
        // the rebuilt list already looks like this — skip the restore.
        assert!(is_noop(&capture(vec![], None)));
        // A selection always needs restoring, even at the top of the list.
        assert!(!is_noop(&capture(vec![7], None)));
        // An anchor flush against a row's top edge (offset 0.0) is NOT a
        // no-op — the row it anchors can be far down the list.
        assert!(!is_noop(&capture(vec![], Some((50, 0.0)))));
        assert!(!is_noop(&capture(vec![], Some((50, 12.0)))));
    }

    #[test]
    fn capture_bundles_selection_and_anchor_verbatim() {
        let anchor = ReloadAnchor {
            selected_ids: vec![1, 2],
            anchor: Some((1, 3.0)),
        };
        assert_eq!(capture(vec![1, 2], Some((1, 3.0))), anchor);
    }
}
