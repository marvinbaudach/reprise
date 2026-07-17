//! Query reload and source/filter transitions for TrackList.
//!
//! ## TAG-1: reload is navigation-neutral
//!
//! `reload()` is called by ~15 sites — sort clicks, rating edits, DnD
//! reorders, deletes, the tag editor's save, watcher reconcile, scans, and
//! more. Before TAG-1, every one of them silently reset the table's
//! selection and scroll to nothing: `TrackListModel::set_query_browsed`
//! fires `items_changed(0, old_total, new_total)`, which GTK's default
//! selection-model handling reads as "everything from position 0 is gone" —
//! collapsing the whole selection, not just rows that actually vanished.
//!
//! `reload()` now captures a [`reload_restore::ReloadAnchor`] (selection by
//! track id, scroll by a track-id + offset anchor — never a raw pixel value,
//! which would point at the wrong row after a resort) before the swap and
//! restores it after, via `capture_reload_anchor`/`restore_reload_anchor`
//! below. Restoring resolves ids to positions against `Shared::
//! current_view_ids()` — a sorted full-table query, since `TrackListModel`
//! deliberately holds no id list of its own (it windows rows from SQL). That
//! is why an untouched list (nothing selected, scrolled to the top) captures
//! no anchor at all and skips the restore: watcher reconciles and scan
//! progress fire `reload()` in bursts on lists nobody is looking at, and
//! those must not pay for a query whose result would change nothing. A caller that genuinely *wants* the old reset-to-nothing behavior
//! must ask for it explicitly (`shared.selection.unselect_all()` or a
//! `clear_selection()` helper) before calling `reload()` — see the TAG-1
//! commit's message for the sweep across all callers (none currently need
//! this; deleted/removed ids already fall out of the selection silently,
//! which is the correct behavior, not a reset).
//!
//! `set_source_and_reload` is a special case: it already owns cross-source
//! selection/scroll memory via `view_state_memory` (NAV-5). Running TAG-1's
//! generic id-based restore on TOP of a genuine source switch would use a
//! stale anchor from the *old* source's id space against the *new* source's
//! ids — usually a harmless no-op (the old anchor id won't exist in an
//! unrelated source), but not always: if the same track id happens to exist
//! in both sources, TAG-1's restore could act on a coincidence, then race
//! NAV-5's own (correct) idle-scheduled scroll restore. So a genuine source
//! change (`old_source != source`) skips TAG-1's wrapper entirely and calls
//! the bare query (`run_query`) directly — `view_state_memory` wins on a
//! location change, exactly as for any other NAV-5-governed transition. A
//! same-source call (e.g. a redundant `set_source` to the already-active
//! source) has nothing for NAV-5 to do and gets TAG-1's restore like any
//! other `reload()` caller.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio::prelude::*;
use gtk4::prelude::*;

use crate::ui::browse_filter_count;
use crate::ui::track_list::reload_restore::{self, ReloadAnchor};
use crate::ui::track_list::track_list_empty_state::{apply_empty_state, empty_state_for};
use crate::ui::track_list::Shared;
use crate::ui::track_list_sort::resolve_sort_on_switch;
use reprise_core::queries::BrowseFilter;
use reprise_core::view_source::ViewSource;

/// How many idle-callback rounds the scroll restore waits for the rebuilt
/// list to gain usable geometry before giving up — mirrors `view_state_
/// memory`'s identical constant (NAV-5), which faces the same "freshly
/// repopulated `ColumnView` doesn't have adjustment geometry until the next
/// allocation pass" issue.
const SCROLL_RESTORE_MAX_ATTEMPTS: u8 = 8;

fn source_snapshot(source: &RefCell<ViewSource>) -> ViewSource {
    source.borrow().clone()
}

/// The track ids currently selected, read directly off the selection bitset
/// (bounded by however many rows the user actually selected — not by the
/// library's total size).
fn selected_ids_before_swap(shared: &Shared) -> Vec<i64> {
    let bitset = shared.selection.selection();
    let Some((mut iter, first)) = gtk4::BitsetIter::init_first(&bitset) else {
        return Vec::new();
    };
    std::iter::once(first)
        .chain(iter.by_ref())
        .filter_map(|position| shared.model.track_at(position).map(|track| track.id))
        .collect()
}

/// Approximates the uniform row height from the adjustment's total content
/// height over the row count — the same technique `current_track_selection::
/// centered_scroll_value` uses for the "jump to now playing" center (NAV-9):
/// `GtkColumnView` rows are uniform height by design, and there is no
/// per-row height API to query instead.
fn row_height(column_view: &gtk4::ColumnView, n_rows: u32) -> Option<f64> {
    if n_rows == 0 {
        return None;
    }
    let adjustment = gtk4::prelude::ScrollableExt::vadjustment(column_view)?;
    let upper = adjustment.upper();
    (upper > 0.0).then(|| upper / f64::from(n_rows))
}

/// Captures the pre-swap `ReloadAnchor`. The anchor row is resolved through
/// a single `track_at` lookup at the viewport-top index rather than by
/// scanning the whole old model into an id array — `TrackListModel` lazily
/// windows its rows from SQL (see that module's doc comment), so iterating
/// every position here would force-load an entire library on every reload.
fn capture_reload_anchor(shared: &Shared) -> ReloadAnchor {
    let selected = selected_ids_before_swap(shared);
    let old_total = shared.model.n_items();
    let scroll_value = gtk4::prelude::ScrollableExt::vadjustment(&shared.column_view)
        .map_or(0.0, |adjustment| adjustment.value());
    // An untouched list (nothing selected, sitting at the top) records no
    // anchor: the rebuilt list is already at the top, so there is nothing to
    // put back, and `restore_reload_anchor` can then skip resolving the id
    // list entirely (see `reload_restore::is_noop`).
    if selected.is_empty() && scroll_value == 0.0 {
        return ReloadAnchor::default();
    }
    let anchor = row_height(&shared.column_view, old_total).and_then(|height| {
        let index = (scroll_value / height).floor().max(0.0) as u32;
        shared
            .model
            .track_at(index)
            .map(|track| (track.id, scroll_value - f64::from(index) * height))
    });
    reload_restore::capture(selected, anchor)
}

/// Restores the captured selection synchronously (the new model's rows
/// exist as soon as `set_query_browsed` returned) and schedules the scroll
/// restore on idle, since a freshly rebuilt list needs at least one
/// allocation pass before its adjustment reports usable geometry.
fn restore_reload_anchor(shared: &Shared, captured: &ReloadAnchor) {
    // Resolving positions costs a sorted full-table id query; skip it when
    // the capture side already established there is nothing to put back.
    if reload_restore::is_noop(captured) {
        return;
    }
    let current_ids = shared.current_view_ids();
    let positions = reload_restore::positions_for_ids(&captured.selected_ids, &current_ids);
    shared.selection.unselect_all();
    for position in positions {
        shared.selection.select_item(position, false);
    }
    schedule_scroll_restore(
        shared.column_view.clone(),
        captured.anchor,
        current_ids,
        SCROLL_RESTORE_MAX_ATTEMPTS,
    );
}

fn schedule_scroll_restore(
    column_view: gtk4::ColumnView,
    anchor: Option<(i64, f64)>,
    current_ids: Vec<i64>,
    attempts: u8,
) {
    if anchor.is_none() || current_ids.is_empty() {
        return;
    }
    gtk4::glib::idle_add_local_once(move || {
        let Some(adjustment) = gtk4::prelude::ScrollableExt::vadjustment(&column_view) else {
            return;
        };
        let (upper, page) = (adjustment.upper(), adjustment.page_size());
        if upper > page {
            let height = upper / current_ids.len() as f64;
            if let Some(target) = reload_restore::scroll_target(anchor, &current_ids, height, page)
            {
                adjustment.set_value(target);
            }
        } else if attempts > 0 {
            schedule_scroll_restore(column_view, anchor, current_ids, attempts - 1);
        }
    });
}

/// Sets `shared.filter` and reloads — the one place that mutates the filter
/// before reloading, shared by `TrackList::set_filter` (the typed-search
/// path, reached via `window.rs`'s debounce timer) and the
/// `REPRISE_SMOKE_FILTER` dev hook (`arm_smoke_filter`), so both apply a new
/// filter through the identical code path.
pub(in crate::ui) fn set_filter_and_reload(shared: &Rc<Shared>, text: &str) {
    *shared.filter.borrow_mut() = text.to_string();
    reload(shared);
}

/// Sets `shared.source` and reloads — the one place that mutates the source
/// before reloading, shared by `TrackList::set_source` and the `REPRISE_
/// SMOKE_SOURCE` dev hook (`arm_smoke_source`), so both switch sources
/// through the identical code path.
///
/// Also resolves `shared.sort` via `resolve_sort_on_switch` (CRITICAL fix,
/// review round 1; see that function for the full matrix): without this,
/// switching to a `Playlist` source reloaded with whatever sort was
/// already active (`SortState::default()`'s artist/asc on first switch),
/// never the playlist's own `pt.position` order — the `"playlist_order"`
/// sentinel existed in `queries.rs`'s whitelist but was only ever
/// exercised by that module's own unit tests, never by the live UI path.
/// A column-header click (`on_sorter_changed`) still overrides this
/// temporarily, exactly as before.
pub(in crate::ui) fn set_source_and_reload(shared: &Rc<Shared>, source: ViewSource) {
    // NAV-5: capture the leaving source's scroll/selection BEFORE the model
    // is replaced below; restore the entering source's remembered state
    // after the query rebuilt it. Same-source calls are plain reloads and
    // deliberately skip both halves.
    let old_source = source_snapshot(&shared.source);
    super::view_state_memory::remember_on_leave(shared, &old_source, &source);
    // Hoisted so the `sort` borrow ends before the `borrow_mut` below.
    let new_sort = resolve_sort_on_switch(&shared.sort.borrow(), &source);
    *shared.sort.borrow_mut() = new_sort;
    *shared.source.borrow_mut() = source;
    let source = source_snapshot(&shared.source);
    // TAG-1 vs NAV-5: a genuine source switch hands selection/scroll
    // restoration entirely to `view_state_memory` below, so it calls the
    // bare `run_query` and skips TAG-1's generic id/anchor restore — running
    // both would let a stale anchor from the *old* source's id space act on
    // the *new* source (see this module's doc comment). A same-source call
    // has nothing for NAV-5 to do, so it goes through `reload` and gets
    // TAG-1's restore like any other caller.
    //
    // The browse bar's visibility used to be toggled here; FIL made the
    // filter row a permanent header, so that call is intentionally gone.
    if old_source == source {
        reload(shared);
    } else {
        run_query(shared);
        let current_ids = shared.current_view_ids();
        super::view_state_memory::restore_on_attach(shared, &source, &current_ids);
    }
}

/// Re-runs the query against the current source/sort/filter state via
/// `TrackListModel::set_query`, then restores the pre-swap selection/scroll
/// anchor (TAG-1) — see this module's doc comment. Every caller except
/// `set_source_and_reload`'s source-switch branch goes through here.
pub(in crate::ui) fn reload(shared: &Rc<Shared>) {
    let captured = capture_reload_anchor(shared);
    run_query(shared);
    restore_reload_anchor(shared, &captured);
}

/// The bare query/model-swap/empty-state work, with no selection/scroll
/// handling of its own — see `reload`'s and `set_source_and_reload`'s doc
/// comments for who wraps this and why.
fn run_query(shared: &Rc<Shared>) {
    let sort = shared.sort.borrow().clone();
    let filter = shared.filter.borrow().clone();
    let source = shared.source.borrow().clone();
    let browse = if matches!(source, ViewSource::Library) {
        shared.browse_filter.borrow().clone()
    } else {
        BrowseFilter::default()
    };
    let has_filter = !filter.trim().is_empty() || !browse.is_empty();

    let is_queue = matches!(source, ViewSource::Queue);
    let queue_ids = if is_queue {
        let queue_model = (shared.queue_ids_provider)();
        *shared.queue_sections.borrow_mut() = queue_model.sections.clone();
        shared
            .model
            .set_sections(super::queue_sections::section_ranges(&queue_model.sections));
        queue_model.ids
    } else {
        shared.queue_sections.borrow_mut().clear();
        shared.model.set_sections(Vec::new());
        Vec::new()
    };
    super::queue_sections::apply_queue_header_factory(shared, is_queue);

    shared.model.set_query_browsed(
        &source,
        &sort.field,
        &sort.dir,
        &filter,
        &browse,
        &queue_ids,
    );

    // Stage 3 Task 8: the ImportErrors source's rows live in `import_errors_
    // view`, not `shared.model` (which `queries.rs` always resolves to an
    // empty window/count for this source — see its module doc's `ImportErrors`
    // section) — so its row count comes from refreshing that panel instead.
    let count = if matches!(source, ViewSource::ImportErrors) {
        shared.import_errors_view.refresh()
    } else {
        shared.model.n_items() as usize
    };
    browse_filter_count::update(
        &shared.browse_bar,
        &shared.conn,
        &source,
        count,
        &filter,
        &browse,
        &queue_ids,
    );
    apply_empty_state(shared, empty_state_for(count, has_filter, &source));

    tracing::info!(
        count,
        field = %sort.field,
        dir = %sort.dir,
        filter = %filter,
        ?browse,
        source = %source.label(),
        "query matched {count} tracks"
    );

    (shared.on_reload)(&source, count, &filter, &browse);
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use reprise_core::view_source::ViewSource;

    #[test]
    fn source_snapshot_releases_the_borrow_before_reentrant_work() {
        let source = RefCell::new(ViewSource::Library);

        let snapshot = super::source_snapshot(&source);
        *source.borrow_mut() = ViewSource::Queue;

        assert!(matches!(snapshot, ViewSource::Library));
        assert!(matches!(*source.borrow(), ViewSource::Queue));
    }
}
