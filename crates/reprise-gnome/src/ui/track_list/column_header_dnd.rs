//! Custom header drag-reorder for the track-list `ColumnView`.
//!
//! ## Why this exists
//!
//! GTK 4.22's native column-header drag-reorder is broken: `GtkColumnViewTitle`'s
//! own click gesture (`gtkcolumnviewtitle.c`'s `click_pressed_cb`) unconditionally
//! claims the event sequence on *press*, in the header title widget's own
//! target/bubble-phase `GtkGestureClick`. `GtkColumnView`'s built-in reorder
//! gesture (`gtkcolumnview.c`'s `self->drag_gesture`) lives one level up, on the
//! internal header row widget, in the *capture* phase — but it only claims the
//! sequence lazily, once `header_drag_update` sees the drag exceed GTK's
//! click-vs-drag threshold. Because the title's claim happens first (an
//! unconditional claim on press always wins the race against a threshold-gated
//! one), every header drag gets cancelled before its threshold check can ever
//! fire: the pointer release lands back in the title's own gesture as a plain
//! click, which is why a header "drag" always resorts the column instead of
//! moving it. Verified against a minimal stock-GTK (no Reprise code) Python
//! repro and against this app's own header directly; unfixed on GNOME's GTK
//! `main` as of 2026-07.
//!
//! The fix: attach our *own* `GestureDrag` directly to the `ColumnView` (a
//! parent of the internal header row, so our capture-phase handler runs before
//! either of GTK's), and claim the sequence ourselves on press, before the
//! title's gesture ever gets a look at it. That means we now own everything a
//! header press can mean:
//!
//! - a plain click re-sorts — [`activate_sort_click`] reimplements
//!   `GtkColumnViewTitle`'s own `activate_sort` against the public
//!   `ColumnView`/`ColumnViewSorter` API;
//! - a drag past the threshold reorders columns — [`live_swap_towards`], using
//!   the `ColumnView`'s public `remove_column`/`insert_column`, which the
//!   app's `wire_order_persistence` listener already picks up exactly as if it
//!   were GTK's own reorder (see `column_layout.rs`);
//! - a press inside the resize zone at either title edge is left unclaimed
//!   entirely, so GTK's own resize gesture (independent of `reorderable`)
//!   keeps working exactly as before.
//!
//! `column_layout.rs` sets `set_reorderable(false)` right where this module is
//! wired in — not because reorder should be off, but because the native path
//! is dead code that must *stay* off: if a future GTK release fixes the
//! title/header claim race, `reorderable(true)` would let GTK's own gesture
//! start claiming sequences again too, double-handling every drag alongside
//! this module's gesture.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio::prelude::*;
use gtk4::prelude::*;

/// Horizontal movement (px) before a header press is treated as a drag rather
/// than a click — mirrors the feel of GTK's own `gtk_drag_check_threshold_
/// double` gate in `header_drag_update`, without depending on it.
const DRAG_THRESHOLD_PX: f64 = 8.0;

/// Distance (px) from a title's own left/right edge that counts as "over the
/// column-boundary resize handle" — inside this band we leave the press
/// unclaimed so GTK's own resize gesture (on the internal header row widget,
/// unaffected by `reorderable`) still gets it.
const RESIZE_ZONE_PX: f64 = 6.0;

/// Matches GTK's own convention (`gtkcolumnview.c`'s `header_drag_update`/
/// `header_drag_end` add/remove this exact class on the dragged title) so any
/// `.dnd` styling the active theme already ships still applies.
const DND_CSS_CLASS: &str = "dnd";

/// State stashed between `drag-begin` and `drag-end`/`cancel` for a header
/// press that landed on a draggable title (i.e. not the resize zone).
struct DragState {
    /// The column under the press. Re-looked-up by identity (not by index)
    /// on every subsequent event, since a live swap changes every index.
    dragged_column: gtk4::ColumnViewColumn,
    /// Set once the horizontal drag threshold is crossed. A state that never
    /// reaches this by `drag-end` was a plain click, not a drag — see
    /// [`handle_drag_end`].
    dragging: bool,
}

/// One header title widget paired with its owning column and current
/// scroll-aware horizontal bounds (relative to `view`), snapshotted fresh via
/// [`header_titles`] every time geometry is needed — a live swap changes the
/// header row's children, so nothing here is safe to cache across a mutation.
struct HeaderTitle {
    column: gtk4::ColumnViewColumn,
    widget: gtk4::Widget,
    left: f64,
    right: f64,
    /// A hidden column still has a title widget (see the module-level
    /// invariant below) but it is neither hit-testable nor a valid swap
    /// neighbor.
    visible: bool,
}

/// Snapshots every title widget in the `ColumnView`'s internal header row,
/// paired with the `ColumnViewColumn` at the same position in `view.columns()`
/// — the header row's children are the per-column title widgets in model
/// order, one each, including hidden columns (their title just has zero
/// visible extent), so a straight index zip is exact.
fn header_titles(view: &gtk4::ColumnView) -> Vec<HeaderTitle> {
    let Some(header) = view.first_child() else {
        return Vec::new();
    };
    let columns = view.columns();
    let mut result = Vec::with_capacity(columns.n_items() as usize);
    let mut child = header.first_child();
    let mut index: u32 = 0;
    while let Some(widget) = child {
        let next = widget.next_sibling();
        if let Some(column) = columns
            .item(index)
            .and_then(|item| item.downcast::<gtk4::ColumnViewColumn>().ok())
        {
            let bounds = widget.compute_bounds(view);
            let (left, right) = bounds.map_or((0.0, 0.0), |rect| {
                let left = f64::from(rect.x());
                (left, left + f64::from(rect.width()))
            });
            let visible = widget.is_visible() && (right - left) > 0.0;
            result.push(HeaderTitle {
                column,
                widget,
                left,
                right,
                visible,
            });
        }
        child = next;
        index += 1;
    }
    result
}

/// The visible title whose horizontal span contains `x`, if any.
fn title_at(titles: &[HeaderTitle], x: f64) -> Option<usize> {
    titles
        .iter()
        .position(|title| title.visible && x >= title.left && x < title.right)
}

/// The next visible title in `titles` in the given direction from `from`
/// (exclusive), skipping over any hidden columns in between — the adjacent
/// swap partner for a live drag past `from`.
fn adjacent_visible_neighbor(titles: &[HeaderTitle], from: usize, forward: bool) -> Option<usize> {
    if forward {
        titles
            .iter()
            .enumerate()
            .skip(from + 1)
            .find(|(_, title)| title.visible)
            .map(|(index, _)| index)
    } else {
        titles
            .iter()
            .enumerate()
            .take(from)
            .rev()
            .find(|(_, title)| title.visible)
            .map(|(index, _)| index)
    }
}

fn find_title_widget(
    view: &gtk4::ColumnView,
    column: &gtk4::ColumnViewColumn,
) -> Option<gtk4::Widget> {
    header_titles(view)
        .into_iter()
        .find(|title| title.column == *column)
        .map(|title| title.widget)
}

fn mark_dragging(view: &gtk4::ColumnView, column: &gtk4::ColumnViewColumn) {
    if let Some(widget) = find_title_widget(view, column) {
        widget.add_css_class(DND_CSS_CLASS);
    }
}

fn unmark_dragging(view: &gtk4::ColumnView, column: &gtk4::ColumnViewColumn) {
    if let Some(widget) = find_title_widget(view, column) {
        widget.remove_css_class(DND_CSS_CLASS);
    }
}

/// True when `y` (in `view`-local coordinates) falls inside the header row —
/// the header is always the `ColumnView`'s first child, flush at the top.
fn is_within_header(y: f64, header_height: f64) -> bool {
    header_height > 0.0 && y <= header_height
}

/// True when `x` sits within [`RESIZE_ZONE_PX`] of either edge of a title's
/// own `[left, right)` bounds — the column-boundary resize handle band that
/// must be left for GTK's own resize gesture rather than claimed here.
fn is_in_resize_zone(x: f64, left: f64, right: f64) -> bool {
    (x - left) <= RESIZE_ZONE_PX || (right - x) <= RESIZE_ZONE_PX
}

/// The `ColumnView.columns()` index to pass to `insert_column` right after
/// `remove_column(dragged)` has already run, so the dragged column lands
/// exactly where `neighbor_index` was — both indices read from the *same*
/// pre-removal snapshot. Removing `dragged_index` shifts every index above it
/// down by one, so naively reusing `neighbor_index` unadjusted looks wrong at
/// first glance; it is nonetheless correct in both directions:
///
/// - forward (`dragged_index < neighbor_index`): the neighbor's own index
///   also shifts down by one post-removal, and the dragged column must land
///   right *after* that shifted slot (`(neighbor_index - 1) + 1`) — the two
///   shifts cancel out, so the original `neighbor_index` is already right.
/// - backward (`dragged_index > neighbor_index`): removal never touches an
///   index below it, so the neighbor's index is unchanged, and the dragged
///   column must land right *before* it — again exactly `neighbor_index`.
///
/// See this module's tests for both directions applied to a real reordered
/// sequence, not just the returned number.
fn post_removal_insert_index(dragged_index: usize, neighbor_index: usize) -> usize {
    debug_assert_ne!(
        dragged_index, neighbor_index,
        "a column cannot be its own swap neighbor"
    );
    neighbor_index
}

/// True once `pointer_x` has crossed the midpoint of a neighbor spanning
/// `[neighbor_left, neighbor_right)`, in the given direction — forward means
/// the pointer must be past (greater than) the midpoint, backward means
/// before (less than) it.
fn crossed_neighbor_midpoint(
    pointer_x: f64,
    neighbor_left: f64,
    neighbor_right: f64,
    forward: bool,
) -> bool {
    let midpoint = (neighbor_left + neighbor_right) / 2.0;
    if forward {
        pointer_x > midpoint
    } else {
        pointer_x < midpoint
    }
}

/// Matches GTK's own `GtkColumnViewTitle::activate_sort` toggle rule: a click
/// on the already-primary-sorted column flips its direction; a click on any
/// other sortable column resets to ascending (dropping whatever direction it
/// last had).
fn next_sort_order(is_primary_column: bool, current_order: gtk4::SortType) -> gtk4::SortType {
    if !is_primary_column {
        return gtk4::SortType::Ascending;
    }
    match current_order {
        gtk4::SortType::Descending => gtk4::SortType::Ascending,
        _ => gtk4::SortType::Descending,
    }
}

/// Removes `dragged` from `view` and reinserts it in `neighbor_index`'s old
/// slot (see [`post_removal_insert_index`]) — one adjacent step. Fires the
/// `ColumnView.columns()` `items-changed` signal that `column_layout.rs`'s
/// `wire_order_persistence` listener already stores under the same setting
/// GTK's own (broken) reorder used to.
fn swap_columns(view: &gtk4::ColumnView, dragged_index: usize, neighbor_index: usize) {
    let columns = view.columns();
    let Some(dragged) = columns
        .item(dragged_index as u32)
        .and_then(|item| item.downcast::<gtk4::ColumnViewColumn>().ok())
    else {
        tracing::warn!(
            dragged_index,
            "header drag: dragged column vanished mid-drag; skipping this swap"
        );
        return;
    };
    view.remove_column(&dragged);
    let insert_at = post_removal_insert_index(dragged_index, neighbor_index);
    view.insert_column(insert_at as u32, &dragged);
}

/// Live-swaps `dragged_column` one adjacent step at a time towards
/// `pointer_x`, re-snapshotting header geometry after every swap (the header
/// row's children order just changed), until neither neighbor's midpoint is
/// crossed any more. Bounded by the column count so a pathological input can
/// never spin forever — each swap moves the dragged column exactly one step,
/// so a full traversal is always enough to settle.
fn live_swap_towards(
    view: &gtk4::ColumnView,
    dragged_column: &gtk4::ColumnViewColumn,
    pointer_x: f64,
) {
    let max_steps = view.columns().n_items() as usize;
    for _ in 0..max_steps {
        let titles = header_titles(view);
        let Some(dragged_index) = titles
            .iter()
            .position(|title| title.column == *dragged_column)
        else {
            return;
        };

        if let Some(forward_index) = adjacent_visible_neighbor(&titles, dragged_index, true) {
            let neighbor = &titles[forward_index];
            if crossed_neighbor_midpoint(pointer_x, neighbor.left, neighbor.right, true) {
                swap_columns(view, dragged_index, forward_index);
                continue;
            }
        }
        if let Some(backward_index) = adjacent_visible_neighbor(&titles, dragged_index, false) {
            let neighbor = &titles[backward_index];
            if crossed_neighbor_midpoint(pointer_x, neighbor.left, neighbor.right, false) {
                swap_columns(view, dragged_index, backward_index);
                continue;
            }
        }
        return;
    }
}

/// Reimplements `GtkColumnViewTitle::activate_sort` (see the module doc's
/// "a plain click re-sorts" point) against the public API: a column with no
/// sorter (e.g. Cover) does nothing; otherwise toggle/reset via
/// [`next_sort_order`] and apply through `ColumnView::sort_by_column`, which
/// `track_list_sort::wire_sort_clicks`'s existing `ColumnViewSorter` listener
/// picks up exactly like a native header click.
fn activate_sort_click(view: &gtk4::ColumnView, column: &gtk4::ColumnViewColumn) {
    if column.sorter().is_none() {
        return;
    }
    let Some(sorter) = view.sorter() else {
        return;
    };
    let Some(cv_sorter) = sorter.downcast_ref::<gtk4::ColumnViewSorter>() else {
        return;
    };
    let is_primary = cv_sorter.primary_sort_column().as_ref() == Some(column);
    let next_order = next_sort_order(is_primary, cv_sorter.primary_sort_order());
    view.sort_by_column(Some(column), next_order);
}

fn handle_drag_begin(
    gesture: &gtk4::GestureDrag,
    view: &gtk4::ColumnView,
    state: &Rc<RefCell<Option<DragState>>>,
    x: f64,
    y: f64,
) {
    *state.borrow_mut() = None;

    let Some(header) = view.first_child() else {
        return;
    };
    if !is_within_header(y, f64::from(header.height())) {
        return;
    }

    let titles = header_titles(view);
    let Some(hit_index) = title_at(&titles, x) else {
        return;
    };
    let hit = &titles[hit_index];
    if is_in_resize_zone(x, hit.left, hit.right) {
        return;
    }

    *state.borrow_mut() = Some(DragState {
        dragged_column: hit.column.clone(),
        dragging: false,
    });
    // Claim now, before the title's own bubble-phase click gesture ever sees
    // this press — see the module doc for why claiming late (GTK's own
    // approach) loses the race.
    gesture.set_state(gtk4::EventSequenceState::Claimed);
}

fn handle_drag_update(
    gesture: &gtk4::GestureDrag,
    view: &gtk4::ColumnView,
    state: &Rc<RefCell<Option<DragState>>>,
    offset_x: f64,
) {
    let Some((start_x, _start_y)) = gesture.start_point() else {
        return;
    };

    let dragged_column = {
        let mut state_ref = state.borrow_mut();
        let Some(drag) = state_ref.as_mut() else {
            return;
        };
        if !drag.dragging {
            if offset_x.abs() <= DRAG_THRESHOLD_PX {
                return;
            }
            drag.dragging = true;
        }
        drag.dragged_column.clone()
    };

    // Re-applied on every update rather than once at the threshold crossing:
    // each live swap detaches/reattaches the dragged column, which may hand
    // it a freshly-built title widget, so the class has to be reasserted on
    // whatever widget currently represents it. `add_css_class` is a no-op if
    // already present, so this costs nothing extra on updates with no swap.
    mark_dragging(view, &dragged_column);

    let pointer_x = start_x + offset_x;
    live_swap_towards(view, &dragged_column, pointer_x);
}

fn handle_drag_end(view: &gtk4::ColumnView, state: &Rc<RefCell<Option<DragState>>>) {
    let Some(drag) = state.borrow_mut().take() else {
        return;
    };
    if drag.dragging {
        unmark_dragging(view, &drag.dragged_column);
        return;
    }
    // The threshold was never crossed: this press-then-release was a plain
    // click, which our early claim in `handle_drag_begin` suppressed from
    // ever reaching the title's own (broken-for-drags-only) click gesture.
    activate_sort_click(view, &drag.dragged_column);
}

fn handle_cancel(view: &gtk4::ColumnView, state: &Rc<RefCell<Option<DragState>>>) {
    if let Some(drag) = state.borrow_mut().take() {
        if drag.dragging {
            unmark_dragging(view, &drag.dragged_column);
        }
    }
}

/// Wires the custom header drag-reorder gesture described in the module doc
/// comment onto `view`. Call once, right after `view.set_reorderable(false)`
/// (see `column_layout.rs::build_columns`); no other state is needed —
/// persistence of a completed reorder happens automatically through the
/// existing `wire_order_persistence` listener on `view.columns()`.
pub(super) fn wire_header_drag(view: &gtk4::ColumnView) {
    let state: Rc<RefCell<Option<DragState>>> = Rc::new(RefCell::new(None));
    let gesture = gtk4::GestureDrag::new();
    gesture.set_button(gtk4::gdk::BUTTON_PRIMARY);
    gesture.set_propagation_phase(gtk4::PropagationPhase::Capture);

    {
        let view = view.clone();
        let state = state.clone();
        gesture.connect_drag_begin(move |gesture, x, y| {
            handle_drag_begin(gesture, &view, &state, x, y);
        });
    }
    {
        let view = view.clone();
        let state = state.clone();
        gesture.connect_drag_update(move |gesture, offset_x, _offset_y| {
            handle_drag_update(gesture, &view, &state, offset_x);
        });
    }
    {
        let view = view.clone();
        let state = state.clone();
        gesture.connect_drag_end(move |_gesture, _offset_x, _offset_y| {
            handle_drag_end(&view, &state);
        });
    }
    {
        let view = view.clone();
        let state = state.clone();
        gesture.connect_cancel(move |_gesture, _sequence| {
            handle_cancel(&view, &state);
        });
    }

    view.add_controller(gesture);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_click_hit_test_matches_only_the_header_band() {
        assert!(is_within_header(0.0, 25.0));
        assert!(is_within_header(25.0, 25.0));
        assert!(!is_within_header(25.1, 25.0));
        assert!(!is_within_header(200.0, 25.0));
        // No measurable header (not yet realized) never counts as a hit.
        assert!(!is_within_header(0.0, 0.0));
    }

    #[test]
    fn resize_zone_covers_both_edges_but_not_the_middle() {
        // A 100px-wide title spanning [200, 300).
        assert!(is_in_resize_zone(200.0, 200.0, 300.0)); // exactly on the left edge
        assert!(is_in_resize_zone(205.9, 200.0, 300.0)); // just inside the left band
        assert!(is_in_resize_zone(300.0, 200.0, 300.0)); // exactly on the right edge
        assert!(is_in_resize_zone(294.1, 200.0, 300.0)); // just inside the right band
        assert!(!is_in_resize_zone(250.0, 200.0, 300.0)); // dead center
        assert!(!is_in_resize_zone(207.0, 200.0, 300.0)); // just past the left band
        assert!(!is_in_resize_zone(293.0, 200.0, 300.0)); // just before the right band
    }

    #[test]
    fn crossed_neighbor_midpoint_respects_direction() {
        // Neighbor spans [300, 400); midpoint is 350.
        assert!(crossed_neighbor_midpoint(360.0, 300.0, 400.0, true));
        assert!(!crossed_neighbor_midpoint(340.0, 300.0, 400.0, true));
        assert!(crossed_neighbor_midpoint(340.0, 300.0, 400.0, false));
        assert!(!crossed_neighbor_midpoint(360.0, 300.0, 400.0, false));
    }

    #[test]
    fn next_sort_order_toggles_the_primary_column_and_resets_any_other() {
        assert_eq!(
            next_sort_order(true, gtk4::SortType::Ascending),
            gtk4::SortType::Descending
        );
        assert_eq!(
            next_sort_order(true, gtk4::SortType::Descending),
            gtk4::SortType::Ascending
        );
        // Clicking a column that is not currently primary always resets to
        // ascending, regardless of whatever direction the *other* primary
        // column was last sorted in.
        assert_eq!(
            next_sort_order(false, gtk4::SortType::Descending),
            gtk4::SortType::Ascending
        );
        assert_eq!(
            next_sort_order(false, gtk4::SortType::Ascending),
            gtk4::SortType::Ascending
        );
    }

    /// Applies `post_removal_insert_index` to a real `Vec` remove+insert
    /// pair and checks the *resulting order*, not just the returned number —
    /// the number alone (`neighbor_index`, unadjusted) looks suspicious
    /// without this, since the doc comment's two directions derive it two
    /// different ways that happen to coincide.
    fn apply_swap(mut order: Vec<char>, dragged_index: usize, neighbor_index: usize) -> Vec<char> {
        let dragged = order.remove(dragged_index);
        let insert_at = post_removal_insert_index(dragged_index, neighbor_index);
        order.insert(insert_at, dragged);
        order
    }

    #[test]
    fn post_removal_insert_index_swaps_adjacent_columns_forward() {
        let order = vec!['A', 'B', 'C', 'D', 'E'];
        // Drag C (index 2) forward past its right neighbor D (index 3).
        let result = apply_swap(order, 2, 3);
        assert_eq!(result, vec!['A', 'B', 'D', 'C', 'E']);
    }

    #[test]
    fn post_removal_insert_index_swaps_adjacent_columns_backward() {
        let order = vec!['A', 'B', 'C', 'D', 'E'];
        // Drag D (index 3) backward past its left neighbor C (index 2).
        let result = apply_swap(order, 3, 2);
        assert_eq!(result, vec!['A', 'B', 'D', 'C', 'E']);
    }

    #[test]
    fn post_removal_insert_index_handles_a_gap_between_dragged_and_neighbor() {
        // A hidden column can sit between the dragged title and the next
        // *visible* one; the helper still has to land the dragged column
        // immediately next to the neighbor it crossed, shifting everything
        // between them by one — not just swap two adjacent slots.
        let order = vec!['A', 'B', 'C', 'D', 'E'];
        // Drag A (index 0) forward past the far visible neighbor D (index 3);
        // B and C (in between) each shift down by one.
        let forward = apply_swap(order.clone(), 0, 3);
        assert_eq!(forward, vec!['B', 'C', 'D', 'A', 'E']);
        // Drag E (index 4) backward past the far visible neighbor B (index 1);
        // C and D (in between) each shift up by one.
        let backward = apply_swap(order, 4, 1);
        assert_eq!(backward, vec!['A', 'E', 'B', 'C', 'D']);
    }
}
