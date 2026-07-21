//! Source/sort/filter guard shared by pointer and keyboard playlist reorder.

use crate::ui::track_list::Shared;
use crate::ui::track_list_sort::PLAYLIST_ORDER_SORT_FIELD;
use reprise_core::view_source::ViewSource;

/// Whether the track list's current state allows a drag-reorder *within* a
/// playlist view (Stage 3 Task 6) — the true-position rule's guard, mirroring
/// `ui::track_actions`'s "Remove from playlist" reasoning one step further:
/// removal can always resolve the true `pt.position` of whatever row is
/// selected (via `Track::playlist_position`), no matter the sort/filter, so it
/// stays correct under any view state. A *reorder* drag has no such escape
/// hatch — dropping a row "between rows 2 and 3" is only a meaningful,
/// unambiguous instruction when the on-screen row order already *is*
/// `pt.position` order (the playlist's own default, the `"playlist_order"`
/// sentinel) with no search filter thinning out which rows are even visible.
/// Under a column-header sort or a live filter, "between the two visible rows"
/// doesn't correspond to any single well-defined target position in the full
/// unsorted/unfiltered list, so this returns `false` and `ui::track_list_dnd`'s
/// reorder-drop handler must treat the drag as a no-op rather than guess.
/// `false` for every non-Playlist source too (Library/Smart/Missing/
/// ImportErrors have no `pt.position` to reorder in the first place; Queue has
/// its own reorder path, gated separately — see that module's doc comment for
/// why Queue never needs this guard at all).
pub(in crate::ui) fn playlist_reorder_allowed(shared: &Shared) -> bool {
    matches!(*shared.source.borrow(), ViewSource::Playlist(_))
        && shared.sort.borrow().field == PLAYLIST_ORDER_SORT_FIELD
        && shared.filter.borrow().trim().is_empty()
}
