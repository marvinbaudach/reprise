//! Drag and drop (Stage 3 Task 6) — split out of `track_list.rs` exactly the
//! way `track_list_context_menu.rs` was (Stage 3 Task 5): same reasoning
//! (keep the owning file from growing without bound), same shape (an
//! `impl`-free sibling module that reaches into `track_list.rs`'s private
//! `Shared` via `pub(in crate::ui)` fields/functions). This module owns three
//! things:
//!
//! 1. **Drag source** (every track-list cell, wired from `track_list.rs`'s
//!    `append_column`/`append_rating_column` factories, same call site as
//!    `track_list_context_menu::wire_context_menu_gesture`): starts a drag
//!    carrying the current selection's track ids (or, if the pressed row
//!    isn't part of the current selection, just that one row — see
//!    [`wire_drag_source`]'s doc comment for why that mirrors the context
//!    menu's own reselect convention in spirit but not in effect).
//! 2. **Drop target: fill a playlist** — `ui::sidebar` wires a `DropTarget` on
//!    each playlist row that calls [`parse_drag_payload`] and
//!    `library::playlists::add_tracks`; this module only supplies the payload
//!    format both sides agree on.
//! 3. **Drop target: reorder** — every track-list cell also carries a
//!    `DropTarget` (this module) that reorders *within* the current list:
//!    `library::playlists::move_position` for a `Playlist` source; for
//!    `Queue`, `queue_row_mapping::reorder_op` resolves the composite-view
//!    drag to `WithinPlayNext` or `PromoteUpNext`; only Play Next rows are
//!    valid targets, dispatched through `PlayerController::
//!    reorder_queue_rows` (injected via `TrackList::set_on_queue_reorder`).
//!    `wire_drop_target`'s `connect_enter` runs the same `reorder_op` (or
//!    `playlist_reorder_allowed`) check *before* showing the drop indicator,
//!    so the indicator only lights up where a drop would actually do
//!    something — it reads `Shared::active_reorder_drag_from`, stashed by
//!    `wire_drag_source`'s `connect_prepare` and cleared on drag end/cancel.
//!
//! ## Content payload format: a comma-joined `String`, not a boxed type
//!
//! The task brief offered two choices: a custom `glib::Boxed` `Vec<i64>`, or
//! a plain `String`. This module uses a `String`: `gdk::ContentProvider::
//! for_value`/`gtk::DropTarget::new(glib::Type::STRING, …)` need no
//! `#[boxed_type]` registration, no extra dependency, and round-trip through
//! a single `GValue` exactly as reliably as a registered boxed type would —
//! for a handful of `i64`s, the simplicity is worth more than the type
//! safety a bespoke boxed type would add. The format is `"<id>,<id>,…|<pos>"`
//! (see [`format_drag_payload`]/[`parse_drag_payload`]): the ids half is what
//! `ui::sidebar`'s "add to playlist" drop needs, and the `|<pos>` half is
//! what a same-list reorder drop needs — see the next section for why a
//! single field can serve both a `Playlist`'s `pt.position` and a `Queue`'s
//! play-order index without the two ever being confused.
//!
//! ## The TRUE-position rule, applied to drag-reorder (binding, Task 5's
//! data-loss bug reprised)
//!
//! Task 5 shipped a bug — since fixed — that mapped a `ColumnView` *view row
//! index* directly onto `playlist_tracks.position`, which is wrong the
//! moment the view is sorted by a column or filtered (see `models::Track::
//! playlist_position`'s doc comment for the full incident). A drag-reorder
//! has the exact same trap, one level worse: a *reorder* drag has no
//! "resolve every selected row's true position independently" escape hatch
//! the way "remove from playlist" does, because "drop between view rows 2
//! and 3" is only a well-defined instruction at all when the view's row
//! order already *is* `pt.position` order. So this module never derives a
//! target position from a raw view index without going through one of:
//!
//! - **Playlist**: [`reorder_position_for_drag`]/the drop handler both read
//!   `Track::playlist_position` via `TrackListModel::track_at` — never the
//!   raw `u32` position `ColumnView` hands out — and both additionally
//!   require `track_list::playlist_reorder_allowed(shared)` (source is
//!   `Playlist`, current sort is the `"playlist_order"` sentinel, no search
//!   filter active) before treating the drag/drop as reorder-eligible at
//!   all. Outside that state, reorder is **disabled** (see that function's
//!   doc comment for the reasoning) — the drag can still carry ids (for
//!   dropping on a *different* playlist's sidebar row), just never a
//!   `reorder_position`.
//! - **Queue**: `queries::query_track_window_queue` ignores sort *and*
//!   filter entirely (pinned by `track_list_model.rs`'s own test,
//!   `set_query_with_queue_source_follows_queue_ids_order`) — the Queue
//!   view's row order is always exactly the composite view GTK renders, so a
//!   view position *is* the row GTK hands out, unconditionally. No sort/
//!   filter guard is needed (see [`reorder_position_for_drag`]'s `Queue`
//!   arm); what still needs resolving is *which* composite-view op a given
//!   `(from, to)` pair means — that's `queue_row_mapping::reorder_op`
//!   (`WithinPlayNext` or `PromoteUpNext`), called by both
//!   [`handle_queue_reorder_drop`] and `wire_drop_target`'s `connect_enter`.
//!
//! Every reorder path also aborts (does nothing) rather than guess if a
//! position can't be resolved — see [`resolve_reorder_target`] and the drop
//! handlers below.
//!
//! ## Single-row reorder-drag only (multi-row deferred)
//!
//! [`resolve_reorder_target`] refuses any payload carrying more than one id:
//! reordering several rows at once via one drag has no single obviously
//! "correct" target-position semantics (does row 2 of 3 land immediately
//! before or after the drop point? do the others keep their relative order?)
//! and the brief explicitly defers it rather than pick an answer under time
//! pressure. A multi-row drag still carries every selected id (so it can
//! still be dropped on a sidebar playlist row to *add* them all), it simply
//! never carries a `reorder_position` — see [`format_drag_payload`]'s doc
//! comment.
//!
//! ## The `REPRISE_SMOKE_DND` hook lives in a sibling module
//!
//! The dev/verification hook that drives this module's drop handlers without
//! a real pointer gesture (`REPRISE_SMOKE_DND=addplaylist:<name>` etc.) is
//! `track_list_dnd_smoke`, not this file — split out (Stage 3 Task 6 review
//! finding #2) purely to keep this file under the project's 800-line rule.
//! It calls [`handle_playlist_reorder_drop`]/[`handle_queue_reorder_drop`]
//! (`pub(in crate::ui)` for exactly that reason), the same functions the real drop
//! targets [`wire_row_dnd`] wires call.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;

use crate::ui::strings;
use crate::ui::track_actions;
use crate::ui::track_list::{playlist_reorder_allowed, reload, show_toast, Shared};
use crate::ui::track_list_context_menu;
use crate::ui::track_list_model::TrackListModel;
use crate::ui::track_list_row_interaction;
use reprise_core::library::playlists;
use reprise_core::view_source::ViewSource;

/// A parsed drag payload — see the module doc's `## Content payload format`
/// section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::ui) struct DragPayload {
    /// Every dragged track's id, in drag order (selection order, or a single
    /// id for a lone-row drag).
    pub ids: Vec<i64>,
    /// `Some(true_position)` only for a single-row drag that started in a
    /// reorder-eligible state (see [`reorder_position_for_drag`]) — `None`
    /// for every multi-row drag and every drag that isn't currently reorder-
    /// eligible.
    pub reorder_position: Option<i64>,
}

/// Formats a drag payload: `ids` joined by commas, then `|`, then either the
/// reorder position or `-` for "not a reorder-eligible drag" — see the
/// module doc's `## Content payload format` section for why a single string
/// field serves both `ui::sidebar`'s "add to playlist" drop (only reads
/// `ids`) and this module's own same-list reorder drop (reads `reorder_
/// position` too).
pub(in crate::ui) fn format_drag_payload(ids: &[i64], reorder_position: Option<i64>) -> String {
    let ids_part = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
    match reorder_position {
        Some(pos) => format!("{ids_part}|{pos}"),
        None => format!("{ids_part}|-"),
    }
}

/// Parses a drag payload built by [`format_drag_payload`]. `None` for
/// anything malformed: no `|` separator, an empty/all-unparseable ids half,
/// or a position half that's neither `-` nor a valid `i64` — a corrupt/
/// foreign payload (e.g. a drop from outside this app that happens to offer
/// a `text/plain` string) must never be guessed at, only rejected. `pub(in crate::ui)`
/// (not private): `ui::sidebar`'s playlist-row drop target parses the exact
/// same payload this module's drag source produces.
pub(in crate::ui) fn parse_drag_payload(payload: &str) -> Option<DragPayload> {
    let (ids_part, pos_part) = payload.split_once('|')?;

    let ids: Vec<i64> = ids_part
        .split(',')
        .map(str::parse::<i64>)
        .collect::<Result<_, _>>()
        .ok()?;
    if ids.is_empty() {
        return None;
    }

    let reorder_position = if pos_part == "-" {
        None
    } else {
        Some(pos_part.parse::<i64>().ok()?)
    };

    Some(DragPayload {
        ids,
        reorder_position,
    })
}

/// Resolves the `reorder_position` to attach to a would-be reorder drag
/// starting at view `position`, given the source it's dragging from and
/// whether that source is currently reorder-eligible (`playlist_reorder_
/// allowed`, resolved by the caller since it needs `Shared`'s private sort/
/// filter state — this function stays free of `Shared` entirely so it's
/// testable with a plain `TrackListModel`, matching `ui::track_actions`'s
/// style). See the module doc's `## The TRUE-position rule` section for why
/// `Playlist` and `Queue` resolve this so differently.
pub(in crate::ui) fn reorder_position_for_drag(
    model: &TrackListModel,
    source: &ViewSource,
    playlist_reorder_allowed: bool,
    position: u32,
) -> Option<i64> {
    match source {
        ViewSource::Playlist(_) if playlist_reorder_allowed => {
            model.track_at(position).and_then(|t| t.playlist_position)
        }
        ViewSource::Queue => Some(i64::from(position)),
        _ => None,
    }
}

/// The `(from, to)` true-position move [`resolve_reorder_target`] decided on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) struct ReorderMove {
    pub from: i64,
    pub to: i64,
}

/// Pure decision of what move (if any) a drop of `payload` onto a row whose
/// true position is `target_true_position` should perform. `None` (no move —
/// the drop handler must then do nothing at all, never guess) when:
/// - `payload` carries more than one id (multi-row reorder-drag is deferred —
///   see the module doc's `## Single-row reorder-drag only` section),
/// - `payload.reorder_position` is `None` (not a reorder-eligible drag at
///   all — a plain "add to playlist" drag, or one that started in a
///   non-reorder-eligible state), or
/// - the resolved `from`/`to` are equal (dropping a row back onto itself).
pub(in crate::ui) fn resolve_reorder_target(
    payload: &DragPayload,
    target_true_position: i64,
) -> Option<ReorderMove> {
    if payload.ids.len() != 1 {
        return None;
    }
    let from = payload.reorder_position?;
    if from == target_true_position {
        return None;
    }
    Some(ReorderMove {
        from,
        to: target_true_position,
    })
}

/// Attaches both the drag source (start a drag from this cell) and the
/// same-list reorder drop target to a freshly-`setup` cell widget — called
/// once per widget instance from `track_list.rs`'s `connect_setup` closures,
/// alongside `track_list_context_menu::wire_context_menu_gesture` (same "one
/// stable `ListItem` handle read fresh at gesture time" reasoning as that
/// function's own doc comment — this module relies on the identical pattern
/// rather than re-explaining it here).
pub(in crate::ui) fn wire_row_dnd(
    widget: &impl IsA<gtk4::Widget>,
    item: &gtk4::ListItem,
    shared: &Rc<Shared>,
) {
    wire_drag_source(widget, item, shared);
    wire_drop_target(widget, item, shared);
}

/// Attaches the `gtk::DragSource` half of [`wire_row_dnd`] — see that
/// function's doc comment.
fn wire_drag_source(widget: &impl IsA<gtk4::Widget>, item: &gtk4::ListItem, shared: &Rc<Shared>) {
    let drag_source = gtk4::DragSource::new();
    drag_source.set_actions(gdk::DragAction::COPY | gdk::DragAction::MOVE);

    // Stashes the just-prepared drag's dragged-row count so `connect_drag_
    // begin` (which has no direct access to the content just built in
    // `connect_prepare`) can size the "N tracks" drag icon correctly. Scoped
    // to this one `DragSource` instance (one per cell widget, built once at
    // `connect_setup` time — same per-widget-instance lifetime as the
    // `ListItem` clone below), so there's no cross-row leakage.
    let last_drag_count: Rc<Cell<usize>> = Rc::new(Cell::new(0));

    {
        let item = item.clone();
        let shared = shared.clone();
        let last_drag_count = last_drag_count.clone();
        drag_source.connect_prepare(move |_source, _x, _y| {
            let position = item.position();
            if position == gtk4::INVALID_LIST_POSITION {
                return None;
            }

            // Dragging a row that's already part of the current selection
            // drags the whole selection; dragging a row that *isn't*
            // selected drags just that one row, leaving the selection
            // untouched — a drag start shouldn't silently blow away a
            // multi-selection the user is still building (unlike a
            // right-click, which GNOME convention says *does* replace an
            // unrelated selection — see `track_list_context_menu`'s `##
            // GNOME right-click selection convention` section; a drag has no
            // equivalent convention pulling the other way, so the more
            // conservative "don't touch the selection" choice was made).
            let selected = track_list_context_menu::current_selection_positions(&shared);
            let dragged_positions: Vec<u32> = if selected.contains(&position) {
                selected
            } else {
                vec![position]
            };

            let ids = track_actions::selected_track_ids(&dragged_positions, &shared.model);
            if ids.is_empty() {
                return None;
            }

            let reorder_position = if dragged_positions.len() == 1 {
                let source = shared.source.borrow().clone();
                let allowed = playlist_reorder_allowed(&shared);
                reorder_position_for_drag(&shared.model, &source, allowed, dragged_positions[0])
            } else {
                None
            };
            shared
                .active_reorder_drag_from
                .set(reorder_position.is_some().then_some(dragged_positions[0]));

            last_drag_count.set(ids.len());
            let payload = format_drag_payload(&ids, reorder_position);
            tracing::debug!(count = ids.len(), ?reorder_position, "track drag prepared");
            Some(gdk::ContentProvider::for_value(&payload.to_value()))
        });
    }

    {
        let last_drag_count = last_drag_count.clone();
        drag_source.connect_drag_begin(move |source, _drag| {
            let count = last_drag_count.get().max(1);
            tracing::debug!(count, "track drag began");
            let label = gtk4::Label::new(Some(&strings::drag_tracks_label(count)));
            label.add_css_class("card");
            label.set_margin_top(6);
            label.set_margin_bottom(6);
            label.set_margin_start(10);
            label.set_margin_end(10);
            // `WidgetPaintable` can paint an unparented, unrealized widget
            // (it drives its own measure/snapshot) — no need to add `label`
            // to any container just to use it as a drag icon.
            let paintable = gtk4::WidgetPaintable::new(Some(&label));
            source.set_icon(Some(&paintable), 0, 0);
        });
    }

    {
        let shared = shared.clone();
        drag_source.connect_drag_end(move |_source, _drag, _delete_data| {
            shared.active_reorder_drag_from.set(None);
        });
    }
    {
        let shared = shared.clone();
        drag_source.connect_drag_cancel(move |_source, _drag, _reason| {
            shared.active_reorder_drag_from.set(None);
            // Keep GTK's default cancel handling (e.g. the snap-back
            // animation) — this handler only clears the stash.
            false
        });
    }

    widget
        .upcast_ref::<gtk4::Widget>()
        .add_controller(drag_source);
}

/// Attaches the `gtk::DropTarget` half of [`wire_row_dnd`] (same-list
/// reorder) — see that function's doc comment. `ui::sidebar`'s "add to
/// playlist" drop target is separate (different widget, different action);
/// this one only ever reorders within whatever list is currently showing.
fn wire_drop_target(widget: &impl IsA<gtk4::Widget>, item: &gtk4::ListItem, shared: &Rc<Shared>) {
    let drop_target = gtk4::DropTarget::new(glib::Type::STRING, gdk::DragAction::MOVE);

    {
        let widget = widget.upcast_ref::<gtk4::Widget>().clone();
        let shared = shared.clone();
        let item = item.clone();
        drop_target.connect_enter(move |_target, _x, _y| {
            let source = shared.source.borrow().clone();
            let from = shared.active_reorder_drag_from.get();
            let to = item.position();
            let eligible = match (from, &source) {
                (Some(from), ViewSource::Queue) if to != gtk4::INVALID_LIST_POSITION => {
                    let sections = shared.queue_sections.borrow();
                    crate::ui::track_list::queue_row_mapping::reorder_op(from, to, &sections)
                        .is_some()
                }
                (Some(from), ViewSource::Playlist(_)) if to != gtk4::INVALID_LIST_POSITION => {
                    playlist_reorder_allowed(&shared) && from != to
                }
                _ => false,
            };
            track_list_row_interaction::set_reorder_indicator(&widget, eligible);
            if eligible {
                tracing::debug!(
                    source = %source.label(),
                    from,
                    to,
                    "reorder drop target entered"
                );
                gdk::DragAction::MOVE
            } else {
                gdk::DragAction::empty()
            }
        });
    }
    {
        let widget = widget.upcast_ref::<gtk4::Widget>().clone();
        drop_target.connect_leave(move |_| {
            track_list_row_interaction::set_reorder_indicator(&widget, false);
        });
    }

    let item = item.clone();
    let shared = shared.clone();
    let drop_widget = widget.upcast_ref::<gtk4::Widget>().clone();
    drop_target.connect_drop(move |_target, value, _x, _y| {
        track_list_row_interaction::set_reorder_indicator(&drop_widget, false);
        let Ok(payload_str) = value.get::<String>() else {
            return false;
        };
        let Some(payload) = parse_drag_payload(&payload_str) else {
            tracing::warn!("track list drop: could not parse drag payload; ignoring");
            return false;
        };
        let target_position = item.position();
        if target_position == gtk4::INVALID_LIST_POSITION {
            return false;
        }
        handle_reorder_drop(&shared, &payload, target_position)
    });

    widget
        .upcast_ref::<gtk4::Widget>()
        .add_controller(drop_target);
}

/// Dispatches a same-list reorder drop to the current source's handler.
/// `_` (Library/Smart/Missing/ImportErrors): no reorder concept, always
/// rejected.
fn handle_reorder_drop(
    shared: &Rc<Shared>,
    payload: &DragPayload,
    target_view_position: u32,
) -> bool {
    let source = shared.source.borrow().clone();
    match source {
        ViewSource::Playlist(playlist_id) => {
            handle_playlist_reorder_drop(shared, playlist_id, payload, target_view_position)
        }
        ViewSource::Queue => handle_queue_reorder_drop(shared, payload, target_view_position),
        _ => false,
    }
}

/// "Reorder within a playlist" drop handler — see the module doc's `## The
/// TRUE-position rule` section. Re-checks `playlist_reorder_allowed` here
/// too (not just at drag-prepare time): the view's sort/filter state could
/// in principle change between a drag starting and its drop landing (a
/// column-header click mid-drag is exotic but not impossible), so the guard
/// is enforced on both ends rather than trusted from one. `pub(in crate::ui)`: also
/// called directly by `track_list_dnd_smoke`'s `reorderplaylist:<from>-<to>`
/// hook.
pub(in crate::ui) fn handle_playlist_reorder_drop(
    shared: &Rc<Shared>,
    playlist_id: i64,
    payload: &DragPayload,
    target_view_position: u32,
) -> bool {
    if !playlist_reorder_allowed(shared) {
        tracing::debug!(
            playlist_id,
            "playlist reorder drop ignored: view is sorted/filtered away from playlist_order"
        );
        return false;
    }
    let Some(target_true_position) = shared
        .model
        .track_at(target_view_position)
        .and_then(|t| t.playlist_position)
    else {
        tracing::warn!(
            playlist_id,
            target_view_position,
            "playlist reorder drop: could not resolve the target row's true position; aborting"
        );
        return false;
    };
    let Some(reorder) = resolve_reorder_target(payload, target_true_position) else {
        return false;
    };
    let (Ok(from), Ok(to)) = (u32::try_from(reorder.from), u32::try_from(reorder.to)) else {
        tracing::warn!(
            playlist_id,
            from = reorder.from,
            to = reorder.to,
            "playlist reorder drop: resolved position out of u32 range; aborting"
        );
        return false;
    };

    let result = {
        let mut conn = shared.conn.borrow_mut();
        playlists::move_position(&mut conn, playlist_id, from, to)
    };
    match result {
        Ok(()) => {
            tracing::info!(
                playlist_id,
                from,
                to,
                "playlist reordered via drag and drop"
            );
            reload(shared);
            true
        }
        Err(error) => {
            tracing::error!(%error, playlist_id, "failed to persist playlist drag-reorder");
            show_toast(shared, &strings::playlist_reorder_failed_toast());
            false
        }
    }
}

/// "Reorder within the queue" drop handler — no `playlist_reorder_allowed`-
/// style guard needed at all (see the module doc's `## The TRUE-position
/// rule` section: the Queue view's row order always *is* the queue's own
/// play order, unconditionally). `pub(in crate::ui)`: also called directly by
/// `track_list_dnd_smoke`'s `reorderqueue:<from>-<to>` hook.
pub(in crate::ui) fn handle_queue_reorder_drop(
    shared: &Rc<Shared>,
    payload: &DragPayload,
    target_view_position: u32,
) -> bool {
    let Some(reorder) = resolve_reorder_target(payload, i64::from(target_view_position)) else {
        return false;
    };
    let (Ok(from), Ok(to)) = (u32::try_from(reorder.from), u32::try_from(reorder.to)) else {
        return false;
    };
    // QUE-8: composite-view coordinates resolve only to a Play Next reorder
    // or promotion of one context row into Play Next. Every other target
    // resolves to `None` and reports failure.
    let op = {
        let sections = shared.queue_sections.borrow();
        crate::ui::track_list::queue_row_mapping::reorder_op(from, to, &sections)
    };
    let Some(op) = op else {
        tracing::debug!(from, to, "queue drag outside QUE-3 rules; rejected");
        return false;
    };

    let callback = shared.on_queue_reorder.borrow().clone();
    match callback {
        Some(callback) => {
            // Stage 3 Task 6 review finding #3: propagate the callback's own
            // report of whether it actually moved anything (`window.rs`'s
            // wiring returns `false` when no player is available at all,
            // exactly like `Queue::move_item`'s own no-op cases) — a
            // degraded no-op must report failure, not success, just because
            // a callback happened to be wired.
            let moved = callback(op);
            if moved {
                tracing::info!(from, to, "queue reordered via drag and drop");
                reload(shared);
            } else {
                tracing::debug!(
                    from,
                    to,
                    "queue reorder drop callback reported no-op; not reloading"
                );
            }
            moved
        }
        None => {
            tracing::warn!(
                "queue reorder drop fired but no on_queue_reorder callback is wired; ignoring"
            );
            false
        }
    }
}

// The `REPRISE_SMOKE_DND` dev/verification hook (three forms: `addplaylist:
// <name>`, `reorderplaylist:<from>-<to>`, `reorderqueue:<from>-<to>`) lives
// in the sibling `track_list_dnd_smoke` module — split out so this file
// stays under the project's 800-line file-size rule (Stage 3 Task 6 review
// finding #2). It calls back into this module's `handle_playlist_reorder_
// drop`/`handle_queue_reorder_drop` (both `pub(in crate::ui)`), the same functions
// the real drop targets wired by [`wire_row_dnd`] call.

#[cfg(test)]
mod tests {
    use super::*;

    // ## format_drag_payload / parse_drag_payload round trips.

    #[test]
    fn round_trips_multi_id_payload_without_reorder_position() {
        let payload = format_drag_payload(&[1, 2, 3], None);
        assert_eq!(payload, "1,2,3|-");
        let parsed = parse_drag_payload(&payload).unwrap();
        assert_eq!(parsed.ids, vec![1, 2, 3]);
        assert_eq!(parsed.reorder_position, None);
    }

    #[test]
    fn round_trips_single_id_payload_with_reorder_position() {
        let payload = format_drag_payload(&[42], Some(7));
        assert_eq!(payload, "42|7");
        let parsed = parse_drag_payload(&payload).unwrap();
        assert_eq!(parsed.ids, vec![42]);
        assert_eq!(parsed.reorder_position, Some(7));
    }

    #[test]
    fn parse_rejects_payload_with_no_separator() {
        assert!(parse_drag_payload("1,2,3").is_none());
    }

    #[test]
    fn parse_rejects_empty_ids_half() {
        assert!(parse_drag_payload("|-").is_none());
    }

    #[test]
    fn parse_rejects_non_numeric_ids() {
        assert!(parse_drag_payload("abc|-").is_none());
    }

    #[test]
    fn parse_rejects_non_numeric_non_dash_position() {
        assert!(parse_drag_payload("1|abc").is_none());
    }

    #[test]
    fn parse_rejects_a_foreign_plain_string_without_a_pipe() {
        // Guards against a drop from outside this app (e.g. a browser tab's
        // dragged link text) being misread as a valid payload.
        assert!(parse_drag_payload("https://example.com/").is_none());
    }

    // ## reorder_position_for_drag

    fn seeded_playlist_model(track_ids_in_order: &[i64]) -> (TrackListModel, i64) {
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        for id in track_ids_in_order {
            conn.execute(
                "INSERT INTO tracks (id, path, title, artist, added_at) VALUES (?1, ?2, ?3, '', 0)",
                rusqlite::params![id, format!("/x/{id}.flac"), format!("Track {id}")],
            )
            .unwrap();
        }
        let conn = std::rc::Rc::new(std::cell::RefCell::new(conn));
        let playlist_id = {
            let c = conn.borrow();
            playlists::create(&c, "P1").unwrap()
        };
        {
            let mut c = conn.borrow_mut();
            playlists::add_tracks(&mut c, playlist_id, track_ids_in_order).unwrap();
        }
        let model = TrackListModel::new(conn);
        model.set_query(
            &ViewSource::Playlist(playlist_id),
            "playlist_order",
            "asc",
            "",
            &[],
        );
        (model, playlist_id)
    }

    #[test]
    fn reorder_position_for_queue_is_always_the_view_position() {
        let (model, _) = seeded_playlist_model(&[10, 20, 30]);
        // Queue never reads `model` at all for this decision — any model
        // works, including one queried over an unrelated playlist.
        assert_eq!(
            reorder_position_for_drag(&model, &ViewSource::Queue, false, 5),
            Some(5)
        );
        assert_eq!(
            reorder_position_for_drag(&model, &ViewSource::Queue, true, 0),
            Some(0)
        );
    }

    #[test]
    fn reorder_position_for_playlist_reads_true_position_when_allowed() {
        let (model, playlist_id) = seeded_playlist_model(&[10, 20, 30]);
        // pt.position 1 holds track id 20 (0-indexed insertion order).
        let pos = reorder_position_for_drag(&model, &ViewSource::Playlist(playlist_id), true, 1);
        assert_eq!(pos, Some(1));
    }

    #[test]
    fn reorder_position_for_playlist_is_none_when_not_allowed() {
        let (model, playlist_id) = seeded_playlist_model(&[10, 20, 30]);
        let pos = reorder_position_for_drag(&model, &ViewSource::Playlist(playlist_id), false, 1);
        assert_eq!(
            pos, None,
            "a sorted/filtered playlist view must never resolve a reorder position"
        );
    }

    #[test]
    fn reorder_position_for_library_is_always_none() {
        let (model, _) = seeded_playlist_model(&[10, 20, 30]);
        assert_eq!(
            reorder_position_for_drag(&model, &ViewSource::Library, true, 0),
            None
        );
    }

    /// Proof this doesn't regress into Task 5's bug: under a divergent
    /// (artist-sorted) view, `reorder_position_for_drag` must still return
    /// the TRUE `pt.position` when `playlist_reorder_allowed` is (falsely)
    /// passed as `true` — asserting the raw view index is never silently
    /// substituted. The caller (`playlist_reorder_allowed`) is what's
    /// actually responsible for passing `false` in that state; this test
    /// pins the position-lookup half of the contract in isolation.
    #[test]
    fn reorder_position_for_playlist_uses_true_position_not_view_index_under_a_sort() {
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        // Insertion order (== pt.position order) is A, B, C; artist-ascending
        // view order is C, A, B (artists Alpha, Zeta, ... chosen to diverge).
        let tracks = [(1, "A", "Zeta"), (2, "B", "Theta"), (3, "C", "Alpha")];
        for (id, title, artist) in tracks {
            conn.execute(
                "INSERT INTO tracks (id, path, title, artist, added_at) VALUES (?1, ?2, ?3, ?4, 0)",
                rusqlite::params![id, format!("/x/{id}.flac"), title, artist],
            )
            .unwrap();
        }
        let playlist_id = playlists::create(&conn, "P1").unwrap();
        let conn = std::rc::Rc::new(std::cell::RefCell::new(conn));
        {
            let mut c = conn.borrow_mut();
            playlists::add_tracks(&mut c, playlist_id, &[1, 2, 3]).unwrap();
        }
        let model = TrackListModel::new(conn);
        model.set_query(&ViewSource::Playlist(playlist_id), "artist", "asc", "", &[]);
        // View row 0 is track C (id 3), whose true pt.position is 2.
        assert_eq!(model.track_at(0).unwrap().id, 3);

        let pos = reorder_position_for_drag(&model, &ViewSource::Playlist(playlist_id), true, 0);
        assert_eq!(
            pos,
            Some(2),
            "must resolve track C's true pt.position (2), not its view index (0)"
        );
    }

    // ## resolve_reorder_target

    #[test]
    fn resolve_reorder_target_single_row_different_position() {
        let payload = DragPayload {
            ids: vec![7],
            reorder_position: Some(2),
        };
        let result = resolve_reorder_target(&payload, 5).unwrap();
        assert_eq!(result, ReorderMove { from: 2, to: 5 });
    }

    #[test]
    fn resolve_reorder_target_rejects_multi_row_payload() {
        let payload = DragPayload {
            ids: vec![7, 8],
            reorder_position: None,
        };
        assert!(resolve_reorder_target(&payload, 5).is_none());
    }

    #[test]
    fn resolve_reorder_target_rejects_a_payload_with_no_reorder_position() {
        let payload = DragPayload {
            ids: vec![7],
            reorder_position: None,
        };
        assert!(resolve_reorder_target(&payload, 5).is_none());
    }

    #[test]
    fn resolve_reorder_target_rejects_dropping_a_row_onto_itself() {
        let payload = DragPayload {
            ids: vec![7],
            reorder_position: Some(3),
        };
        assert!(resolve_reorder_target(&payload, 3).is_none());
    }
}
