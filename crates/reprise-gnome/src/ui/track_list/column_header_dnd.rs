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
//! - a drag past the threshold reorders columns, mirroring the app's own ROW
//!   drag-and-drop idiom (`track_list_row_interaction.rs`'s drop indicator)
//!   rather than moving anything live: `drag-update` only recomputes an
//!   [`InsertionSlot`] from the pointer and toggles a thin accent-colored
//!   marker on whichever title it would land next to ([`update_marker`]); the
//!   dragged title itself just dims ([`mark_drag_source`]). Nothing in
//!   `view.columns()` changes until release — [`perform_drop`] does the one
//!   `remove_column`/`insert_column` call, in `drag-end`, which is also the
//!   only point that fires `column_layout.rs`'s `wire_order_persistence`
//!   listener (exactly once per completed drag);
//! - a press inside the resize zone at either title edge is left unclaimed
//!   entirely, so GTK's own resize gesture (independent of `reorderable`)
//!   keeps working exactly as before.
//!
//! An earlier version of this module moved columns live: the instant the
//! pointer crossed a neighbor's midpoint *during* the drag, it swapped the
//! dragged title with that neighbor right then. Live user testing called this
//! out directly — columns visibly jumping around mid-drag read as "design
//! chaos", every crossing did a real `remove_column`/`insert_column` (a full
//! cell rebuild for that column) plus a settings write, and painting the
//! *whole* dragged column via GTK's own `.dnd` class was far louder than
//! intended. This rework removes all of that: the drag is purely a marker,
//! and the model is mutated exactly once, on release — see [`InsertionSlot`]
//! and [`resolve_drop`] for the math.
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

/// Marks the title the dragged column will land *before* — see [`css`].
const INSERT_BEFORE_CLASS: &str = "reprise-col-insert-before";
/// Marks the last visible title when the drop target is past everything
/// (i.e. the dragged column would land at the very end) — see [`css`].
const INSERT_AFTER_CLASS: &str = "reprise-col-insert-after";
/// Dims the dragged title itself once the drag threshold is crossed — see
/// [`css`]. Deliberately subtle: an earlier version reused GTK's own `.dnd`
/// class here, which paints the *whole* column body, not just the header
/// title, and read as far louder than intended.
const DRAG_SOURCE_CLASS: &str = "reprise-col-drag-source";

/// Column header drag-reorder visuals, installed app-wide by [`super::style`]
/// (`style::mod::app_css`). Mirrors the row drag-and-drop idiom this whole
/// interaction was reworked to match: `track_list_row_interaction.rs`'s
/// `.now-playing-leading` uses the identical `inset 2px 0 0 @accent_color`
/// vertical accent line, and `column_layout_editor.rs`'s before/after drop
/// classes are the same before/after pairing, just oriented for a vertical
/// row list instead of a horizontal column header row.
pub(in crate::ui) fn css() -> String {
    use super::style::tokens::DROP_INDICATOR_THICKNESS;
    format!(
        ".{INSERT_BEFORE_CLASS} {{ box-shadow: inset {DROP_INDICATOR_THICKNESS} 0 0 @accent_color; }}\n\
         .{INSERT_AFTER_CLASS} {{ box-shadow: inset -{DROP_INDICATOR_THICKNESS} 0 0 @accent_color; }}\n\
         .{DRAG_SOURCE_CLASS} {{ opacity: 0.55; }}"
    )
}

/// State stashed between `drag-begin` and `drag-end`/`cancel` for a header
/// press that landed on a draggable title (i.e. not the resize zone).
struct DragState {
    /// The column under the press. Re-looked-up by identity (not by index)
    /// on every subsequent event — nothing is mutated during the drag, but
    /// the header's title widgets can still be recreated out from under us
    /// (e.g. a concurrent visibility change), so identity is the only safe
    /// handle to keep across events.
    dragged_column: gtk4::ColumnViewColumn,
    /// Set once the horizontal drag threshold is crossed. A state that never
    /// reaches this by `drag-end` was a plain click, not a drag — see
    /// [`handle_drag_end`].
    dragging: bool,
    /// The title widget currently carrying an insertion-marker class, paired
    /// with which class it is — so a later change (or the final clear) only
    /// ever removes exactly that one, and so [`update_marker`] can skip
    /// touching CSS at all on a motion event that didn't cross into a new
    /// slot (no per-motion churn).
    marker: Option<(gtk4::Widget, &'static str)>,
}

/// One header title widget paired with its owning column and current
/// scroll-aware horizontal bounds (relative to `view`), snapshotted fresh via
/// [`header_titles`] every time geometry is needed.
struct HeaderTitle {
    column: gtk4::ColumnViewColumn,
    widget: gtk4::Widget,
    left: f64,
    right: f64,
    /// A hidden column still has a title widget (see the module-level
    /// invariant below) but it is neither hit-testable nor a valid drop
    /// target.
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

fn find_title_widget(
    view: &gtk4::ColumnView,
    column: &gtk4::ColumnViewColumn,
) -> Option<gtk4::Widget> {
    header_titles(view)
        .into_iter()
        .find(|title| title.column == *column)
        .map(|title| title.widget)
}

fn mark_drag_source(view: &gtk4::ColumnView, column: &gtk4::ColumnViewColumn) {
    if let Some(widget) = find_title_widget(view, column) {
        widget.add_css_class(DRAG_SOURCE_CLASS);
    }
}

fn unmark_drag_source(view: &gtk4::ColumnView, column: &gtk4::ColumnViewColumn) {
    if let Some(widget) = find_title_widget(view, column) {
        widget.remove_css_class(DRAG_SOURCE_CLASS);
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

/// Where a completed drag would insert the dragged column, expressed
/// relative to the *other* visible titles — computed fresh from the
/// pointer's position; naming a slot never mutates anything by itself. See
/// [`resolve_drop`] for what a slot resolves to as an actual
/// `ColumnView.columns()` index (or "no-op").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InsertionSlot {
    /// Land immediately before the visible title at this model index.
    Before(usize),
    /// Land after every visible title (the pointer is past the last one's
    /// midpoint).
    End,
}

/// Pure, display-free geometry input for [`insertion_slot_for_pointer`] and
/// [`resolve_drop`]'s tests — one entry per header title in model-index
/// order (including hidden columns), mirroring [`HeaderTitle`] without any
/// GTK types so the slot math is testable without a display.
#[derive(Debug, Clone, Copy)]
struct TitleSpan {
    visible: bool,
    left: f64,
    right: f64,
}

impl From<&HeaderTitle> for TitleSpan {
    fn from(title: &HeaderTitle) -> Self {
        TitleSpan {
            visible: title.visible,
            left: title.left,
            right: title.right,
        }
    }
}

/// The insertion slot `pointer_x` currently names: the first VISIBLE title
/// (in model order; hidden ones are skipped entirely, so the pointer can
/// never target one) whose horizontal midpoint sits to the right of
/// `pointer_x` — insert before that one. Past every visible title's
/// midpoint, the slot is [`InsertionSlot::End`].
fn insertion_slot_for_pointer(spans: &[TitleSpan], pointer_x: f64) -> InsertionSlot {
    for (index, span) in spans.iter().enumerate() {
        if !span.visible {
            continue;
        }
        let midpoint = (span.left + span.right) / 2.0;
        if midpoint > pointer_x {
            return InsertionSlot::Before(index);
        }
    }
    InsertionSlot::End
}

/// Resolves an [`InsertionSlot`] (read from a snapshot where the dragged
/// column was still at `dragged_index`) to the actual `ColumnView.columns()`
/// index [`perform_drop`] should pass to `insert_column` — or `None` if
/// dropping there would not change anything (the slot names the dragged
/// column's own current spot), in which case the caller must skip the
/// remove/insert (and show no marker) entirely.
///
/// This is *not* the old adjacent-swap helper's math: "insert before T"
/// needs a different index shift than "swap past T", because the goal is to
/// land immediately before T, not after it:
/// - T after the dragged column (`t > dragged_index`): removing the dragged
///   column shifts T's own index down by one, and the dragged column must
///   land in that now-vacant slot right before it — `t - 1`. If T was
///   `dragged_index`'s immediate next title with nothing (not even a hidden
///   column) between them, `t - 1 == dragged_index`: a true no-op — "release
///   directly after where it already was" does nothing.
/// - T at or before the dragged column (`t <= dragged_index`): removal never
///   touches an index at or below `t`, so `t` itself is already the right
///   landing slot. `t == dragged_index` only when the slot names the dragged
///   column itself ("insert before itself") — also a no-op.
/// - [`InsertionSlot::End`]: the last valid index once the dragged column is
///   removed (`title_count - 1`); a no-op if the dragged column is already
///   the very last title.
fn resolve_drop(dragged_index: usize, slot: InsertionSlot, title_count: usize) -> Option<usize> {
    let target_index = match slot {
        InsertionSlot::Before(t) if t > dragged_index => t - 1,
        InsertionSlot::Before(t) => t,
        InsertionSlot::End => title_count.saturating_sub(1),
    };
    (target_index != dragged_index).then_some(target_index)
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

/// The (widget, css-class) an insertion slot should mark, given whether it
/// actually [`resolve_drop`]s to a real move — `None` for a resolved no-op
/// slot, so the caller shows no marker at all for "drop right where it
/// already is".
fn marker_target(
    titles: &[HeaderTitle],
    slot: InsertionSlot,
    resolved: Option<usize>,
) -> Option<(gtk4::Widget, &'static str)> {
    resolved?;
    match slot {
        InsertionSlot::Before(t) => titles
            .get(t)
            .map(|title| (title.widget.clone(), INSERT_BEFORE_CLASS)),
        InsertionSlot::End => titles
            .iter()
            .rev()
            .find(|title| title.visible)
            .map(|title| (title.widget.clone(), INSERT_AFTER_CLASS)),
    }
}

/// Updates `drag.marker` to `target`, touching CSS only if the (widget,
/// class) pair actually changed since the last call — a no-op on every
/// motion event that doesn't cross into a new slot.
fn apply_marker(drag: &mut DragState, target: Option<(gtk4::Widget, &'static str)>) {
    if drag.marker == target {
        return;
    }
    if let Some((widget, class)) = &drag.marker {
        widget.remove_css_class(class);
    }
    if let Some((widget, class)) = &target {
        widget.add_css_class(class);
    }
    drag.marker = target;
}

fn clear_marker(drag: &mut DragState) {
    apply_marker(drag, None);
}

/// Recomputes the insertion slot for `pointer_x` and updates the marker CSS
/// to match — called on every `drag-update` past the threshold. Never
/// touches `view.columns()`; see [`perform_drop`] for the one move that
/// happens on release.
fn update_marker(
    view: &gtk4::ColumnView,
    drag: &mut DragState,
    dragged_column: &gtk4::ColumnViewColumn,
    pointer_x: f64,
) {
    let titles = header_titles(view);
    let Some(dragged_index) = titles
        .iter()
        .position(|title| title.column == *dragged_column)
    else {
        clear_marker(drag);
        return;
    };
    let spans: Vec<TitleSpan> = titles.iter().map(TitleSpan::from).collect();
    let slot = insertion_slot_for_pointer(&spans, pointer_x);
    let resolved = resolve_drop(dragged_index, slot, titles.len());
    let target = marker_target(&titles, slot, resolved);
    apply_marker(drag, target);
}

/// The one `remove_column`/`insert_column` a completed drag ever performs —
/// called once from `drag-end`, resolving the slot fresh from the pointer's
/// *final* position (the same geometry/resolution logic [`update_marker`]
/// used throughout the drag, so this always agrees with wherever the marker
/// was last shown). A no-op resolution means no call at all: dropping
/// exactly where the marker was pointing is the only way this ever fires,
/// matching the row-drag idiom this mirrors — the move happens on release,
/// not during the drag.
fn perform_drop(view: &gtk4::ColumnView, dragged_column: &gtk4::ColumnViewColumn, pointer_x: f64) {
    let titles = header_titles(view);
    let Some(dragged_index) = titles
        .iter()
        .position(|title| title.column == *dragged_column)
    else {
        return;
    };
    let spans: Vec<TitleSpan> = titles.iter().map(TitleSpan::from).collect();
    let slot = insertion_slot_for_pointer(&spans, pointer_x);
    let Some(target_index) = resolve_drop(dragged_index, slot, titles.len()) else {
        return;
    };
    let columns = view.columns();
    let Some(dragged) = columns
        .item(dragged_index as u32)
        .and_then(|item| item.downcast::<gtk4::ColumnViewColumn>().ok())
    else {
        tracing::warn!(
            dragged_index,
            "header drag: dragged column vanished before drop; skipping the move"
        );
        return;
    };
    view.remove_column(&dragged);
    view.insert_column(target_index as u32, &dragged);
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
        marker: None,
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

    let mut state_ref = state.borrow_mut();
    let Some(drag) = state_ref.as_mut() else {
        return;
    };
    if !drag.dragging {
        if offset_x.abs() <= DRAG_THRESHOLD_PX {
            return;
        }
        drag.dragging = true;
        mark_drag_source(view, &drag.dragged_column);
    }

    let dragged_column = drag.dragged_column.clone();
    let pointer_x = start_x + offset_x;
    update_marker(view, drag, &dragged_column, pointer_x);
}

fn handle_drag_end(
    gesture: &gtk4::GestureDrag,
    view: &gtk4::ColumnView,
    state: &Rc<RefCell<Option<DragState>>>,
    offset_x: f64,
) {
    let Some(mut drag) = state.borrow_mut().take() else {
        return;
    };
    if !drag.dragging {
        // The threshold was never crossed: this press-then-release was a
        // plain click, which our early claim in `handle_drag_begin`
        // suppressed from ever reaching the title's own (broken-for-drags-
        // only) click gesture.
        activate_sort_click(view, &drag.dragged_column);
        return;
    }
    clear_marker(&mut drag);
    unmark_drag_source(view, &drag.dragged_column);

    let Some((start_x, _start_y)) = gesture.start_point() else {
        return;
    };
    perform_drop(view, &drag.dragged_column, start_x + offset_x);
}

fn handle_cancel(view: &gtk4::ColumnView, state: &Rc<RefCell<Option<DragState>>>) {
    let Some(mut drag) = state.borrow_mut().take() else {
        return;
    };
    if drag.dragging {
        clear_marker(&mut drag);
        unmark_drag_source(view, &drag.dragged_column);
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
        gesture.connect_drag_end(move |gesture, offset_x, _offset_y| {
            handle_drag_end(gesture, &view, &state, offset_x);
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
#[path = "column_header_dnd_tests.rs"]
mod tests;
