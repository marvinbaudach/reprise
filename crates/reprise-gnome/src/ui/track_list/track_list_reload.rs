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
//! `set_source_and_reload` is retained for smoke hooks. Live navigation goes
//! through `TrackList::restore_browser_place`: fresh destinations receive a
//! fresh state, while Back/Forward restore the complete router-owned place.

use std::cell::RefCell;
use std::rc::Rc;

use crate::ui::playback::queue_transport::QueueContextWindow;

use gtk4::gio::prelude::*;
use gtk4::prelude::*;

use crate::ui::adjustment_hold::AdjustmentHold;
use crate::ui::browse_filter_count;
use crate::ui::list_geometry::{ListGeometry, RowHeight};
use crate::ui::track_list::reload_restore::{self, ReloadAnchor};
use crate::ui::track_list::track_list_empty_state::{
    apply_empty_state, empty_state_for_availability,
};
use crate::ui::track_list::track_list_model_change::ModelChange;
use crate::ui::track_list::Shared;
use crate::ui::track_list_sort::resolve_sort_on_switch;
use reprise_core::queries::BrowseFilter;
use reprise_core::view_source::ViewSource;

fn observed_row_height(shared: &Shared, n_rows: u32) -> Option<f64> {
    let n_sections = shared.queue_sections.borrow().len();
    ListGeometry::for_view(&shared.column_view)
        .observed_row_height(
            &shared.conn,
            &shared.list_geometry_cache,
            n_rows as usize,
            n_sections,
        )
        .map(crate::ui::list_geometry::RowHeight::pixels)
}

/// SEARCH-9: how many idle rounds `schedule_top_scroll_restore` re-applies its
/// zero. It only has to outlast the one allocation that GTK's own scroll
/// restore rides in on, and every extra round is a round in which the loop
/// cannot tell a re-clamp from the user grabbing the scrollbar — and would
/// snap a deliberate scroll back to the top. Two rounds cover the allocation
/// with one to spare.
const TOP_RESTORE_MAX_ATTEMPTS: u8 = 2;
const SCROLL_ADJUSTMENT_HOLD: std::time::Duration = std::time::Duration::from_millis(250);

#[derive(Clone, Copy)]
pub(in crate::ui) enum ReloadViewport {
    PreserveAnchor,
    CenterPlayingTrack,
    CenterPlayingElsePreSearch,
    /// SEARCH-9: a new result set is read from its top.
    Top,
    /// SEARCH-9: an emptied query returns to `Shared::pre_search.anchor`.
    RestorePreSearch,
}

pub(in crate::ui) fn viewport_after_clearing(
    had_query: bool,
    started_in_search: bool,
) -> ReloadViewport {
    match (had_query, started_in_search) {
        (true, true) => ReloadViewport::CenterPlayingElsePreSearch,
        (true, false) => ReloadViewport::RestorePreSearch,
        (false, _) => ReloadViewport::CenterPlayingTrack,
    }
}

fn filter_change_viewport(
    previous: &str,
    current: &str,
    started_in_search: bool,
) -> ReloadViewport {
    if previous == current {
        ReloadViewport::PreserveAnchor
    } else if current.is_empty() {
        viewport_after_clearing(!previous.is_empty(), started_in_search)
    } else {
        ReloadViewport::Top
    }
}

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

/// The anchor a reveal that has been requested but has not started moving is
/// heading for: the loaded track, centred, in the `(track id, offset)` form
/// the restore resolves against the rebuilt list. Expressing it as an anchor
/// rather than a pixel value is what survives the rows the reload is about to
/// add or drop.
fn pending_reveal_anchor(shared: &Shared, height: RowHeight) -> Option<(i64, f64)> {
    if !shared.track_reveal_pending.get() {
        return None;
    }
    let track_id = shared.playing_track_id.get()?;
    let adjustment = gtk4::prelude::ScrollableExt::vadjustment(&shared.column_view)?;
    let height = height.pixels();
    // `scroll_center::centered_scroll_value` in anchor form: the row's middle
    // on the viewport's middle is its top, minus half a viewport, plus half a
    // row. Its rows-only centring model is tracked separately from anchors.
    Some((track_id, height.mul_add(0.5, -adjustment.page_size() / 2.0)))
}

/// Before the model swap, unlike during restore, the adjustment range and row
/// count describe the same list. Capture that exact quotient so a later stale
/// widget allocation or an assumed CSS floor cannot reinterpret the anchor.
fn capture_row_height(shared: &Shared, old_total: u32) -> Option<RowHeight> {
    if old_total == 0 {
        return None;
    }
    if shared.queue_sections.borrow().is_empty() {
        let adjustment = shared.column_view.vadjustment()?;
        return RowHeight::new(adjustment.upper() / f64::from(old_total));
    }
    observed_row_height(shared, old_total).and_then(RowHeight::new)
}

/// Captures the pre-swap `ReloadAnchor`. The anchor row is resolved through
/// a single `track_at` lookup at the viewport-top index rather than by
/// scanning the whole old model into an id array — `TrackListModel` lazily
/// windows its rows from SQL (see that module's doc comment), so iterating
/// every position here would force-load an entire library on every reload.
pub(in crate::ui) fn capture_reload_anchor(shared: &Shared) -> ReloadAnchor {
    let selected = selected_ids_before_swap(shared);
    let old_total = shared.model.n_items();
    let row_height = capture_row_height(shared, old_total);
    let layout = super::track_list_geometry::layout(shared, row_height, old_total as usize);
    // NAV-10b: a reveal is already under way, so the viewport the user is
    // about to have is its destination — preserving the position the reload
    // *finds* would put the list back where playback just left, and the hold
    // guarding it would then out-write the reveal.
    if let Some(anchor) = row_height.and_then(|height| pending_reveal_anchor(shared, height)) {
        return reload_restore::capture_with_row_height(selected, Some(anchor), row_height);
    }
    // NAV-10b: while the table is gliding to the loaded track, the viewport
    // the user is about to have is the glide's destination, not the frame it
    // happens to be passing through. Anchoring on the live value instead
    // captured a waypoint — and the hold that guards it then wrote that
    // waypoint back, which reads to `ScrollGlide` as a foreign write and ends
    // the glide there. A scan reloads in bursts, so this lands mid-follow
    // routinely.
    let scroll_value = shared.scroll_glide.destination().unwrap_or_else(|| {
        gtk4::prelude::ScrollableExt::vadjustment(&shared.column_view)
            .map_or(0.0, |adjustment| adjustment.value())
    });
    // An untouched list (nothing selected, sitting at the top) records no
    // anchor: the rebuilt list is already at the top, so there is nothing to
    // put back, and `restore_reload_anchor` can then skip resolving the id
    // list entirely (see `reload_restore::is_noop`).
    if selected.is_empty() && scroll_value == 0.0 {
        return ReloadAnchor::default();
    }
    let anchor = layout.and_then(|layout| {
        let (position, offset) = layout.row_at(scroll_value.max(0.0));
        shared
            .model
            .track_at(position)
            .map(|track| (track.id, offset))
    });
    reload_restore::capture_with_row_height(selected, anchor, row_height)
}

/// Restores the captured selection synchronously (the new model's rows
/// exist as soon as `set_query_browsed` returned) and schedules the scroll
/// restore on idle, since a freshly rebuilt list needs at least one
/// allocation pass before its adjustment reports usable geometry.
fn restore_reload_anchor(
    shared: &Rc<Shared>,
    captured: &ReloadAnchor,
    viewport: ReloadViewport,
    hold: Option<&AdjustmentHold>,
    resolved_ids: Option<Vec<i64>>,
) {
    // SEARCH-9: a new result set is read from its top. Doing this before the
    // early return below is what makes the typed-search path cheap — it needs
    // no id list at all, so the sorted full-table query disappears whenever
    // nothing is selected.
    if matches!(viewport, ReloadViewport::Top) {
        schedule_top_scroll_restore(shared.column_view.clone(), TOP_RESTORE_MAX_ATTEMPTS);
    }
    // Resolving positions costs a sorted full-table id query; skip it when
    // the capture side already established there is nothing to put back and
    // the caller did not request a playing-track reveal.
    let reveal_playing_track = matches!(
        viewport,
        ReloadViewport::CenterPlayingTrack | ReloadViewport::CenterPlayingElsePreSearch
    ) && shared.playing_track_id.get().is_some();
    let restores_pre_search = matches!(
        viewport,
        ReloadViewport::RestorePreSearch | ReloadViewport::CenterPlayingElsePreSearch
    ) && shared.pre_search.get().anchor.is_some();
    if reload_restore::is_noop(captured) && !reveal_playing_track && !restores_pre_search {
        if matches!(viewport, ReloadViewport::CenterPlayingElsePreSearch) {
            // A restored query may have no pre-search anchor. Still consume
            // the playback marker when its cleared list has nothing to reveal.
            shared.pre_search.take();
        }
        return;
    }
    let current_ids = resolved_ids.unwrap_or_else(|| shared.current_view_ids());
    select_captured_ids(shared, captured, &current_ids);

    if matches!(
        viewport,
        ReloadViewport::CenterPlayingTrack | ReloadViewport::CenterPlayingElsePreSearch
    ) {
        let playing_track_id = shared.playing_track_id.get();
        if playing_track_id.is_some_and(|track_id| current_ids.contains(&track_id)) {
            if matches!(viewport, ReloadViewport::CenterPlayingElsePreSearch) {
                shared.pre_search.take();
            }
            // The guard is not created here: it belongs to the moment the
            // centered value is known, which is inside the reveal. A guard
            // armed earlier has nothing to defend but the offset the cleared
            // list happens to sit at, and it fights the view over that —
            // measured as a restore that moved through 482.0 twice before it
            // settled. The caller's guard, when it has one, is handed down so
            // the reveal retargets it instead of adding a second.
            super::centered_scroll_restore::schedule(
                shared,
                playing_track_id,
                current_ids,
                hold.cloned(),
            );
            return;
        }
    }

    // SEARCH-9: the search is over — put the user back where it started. A
    // consumed anchor is taken, not copied: the next search captures its own.
    if matches!(
        viewport,
        ReloadViewport::RestorePreSearch | ReloadViewport::CenterPlayingElsePreSearch
    ) {
        let anchor = shared.pre_search.take().anchor;
        super::reload_anchor_scroll::schedule(shared, anchor, None, &current_ids, hold);
        return;
    }

    // `Top` already placed the viewport above; the captured anchor belongs to
    // the pre-filter list and must not pull it back.
    if matches!(viewport, ReloadViewport::Top) {
        return;
    }

    super::reload_anchor_scroll::schedule(
        shared,
        captured.anchor,
        captured.row_height,
        &current_ids,
        hold,
    );
}

/// SEARCH-9: puts the viewport at the top of a freshly filtered list, and keeps
/// it there.
///
/// A single write does not survive. `restore_reload_anchor` runs right after
/// the model swap, while the rebuilt `ColumnView` still carries the *old*
/// allocation; the allocation pass that follows restores GTK's own scroll
/// position — the pre-filter value, clamped to the new and usually much
/// shorter list. A display test caught exactly that: 486 instead of 0, 486
/// being the clamped remains of where the list stood before the query.
///
/// So the zero is re-applied across idle rounds, like the anchor restore next
/// door. This legacy SEARCH-9 path needs to outlast one allocation, not track a
/// moving target. It stops as soon as a round finds the value still at zero —
/// at that point nothing is writing against us any more.
fn schedule_top_scroll_restore(column_view: gtk4::ColumnView, attempts: u8) {
    let Some(adjustment) = gtk4::prelude::ScrollableExt::vadjustment(&column_view) else {
        return;
    };
    let already_settled = adjustment.value() == 0.0;
    crate::ui::scroll_probe::probe("top_restore", &adjustment, 0.0);
    adjustment.set_value(0.0);
    if already_settled || attempts == 0 {
        return;
    }
    gtk4::glib::idle_add_local_once(move || {
        schedule_top_scroll_restore(column_view, attempts - 1);
    });
}

/// Puts the captured selection back on the rebuilt model. Rows the swap
/// dropped simply fall out — see `reload_restore::positions_for_ids`.
fn select_captured_ids(shared: &Shared, captured: &ReloadAnchor, current_ids: &[i64]) {
    if reload_restore::is_noop(captured) {
        return;
    }
    let positions = reload_restore::positions_for_ids(&captured.selected_ids, current_ids);
    shared.selection.unselect_all();
    for position in positions {
        shared.selection.select_item(position, false);
    }
}

/// START-3: selects and centers the loaded track once startup routing has
/// built the restored view.
///
/// Called after `route_to_place`, which is the one moment nothing else owns
/// A no-op when nothing is loaded or the loaded track is not part of the
/// view, preserving that view's own selection and viewport.
pub(in crate::ui) fn center_loaded_track(shared: &Rc<Shared>) {
    let Some(track_id) = shared.playing_track_id.get() else {
        return;
    };
    let current_ids = shared.current_view_ids();
    let Some(position) = current_ids.iter().position(|id| *id == track_id) else {
        tracing::debug!(
            track_id,
            "startup selection skipped: loaded track is not in the restored view"
        );
        return;
    };
    shared.selection.unselect_all();
    shared.selection.select_item(position as u32, false);
    // START-3 needs no guard: nothing swapped the model out from under
    // this view, so no allocation pass is holding an offset to write back.
    super::centered_scroll_restore::schedule(shared, Some(track_id), current_ids, None);
}

/// Sets `shared.filter` and reloads — the one place that mutates the filter
/// before reloading, shared by `TrackList::set_filter` (the typed-search
/// path, reached via `window.rs`'s debounce timer) and the
/// `REPRISE_SMOKE_FILTER` dev hook (`arm_smoke_filter`), so both apply a new
/// filter through the identical code path.
pub(in crate::ui) fn set_filter_and_reload(shared: &Rc<Shared>, text: &str) {
    let previous = shared.filter.borrow().clone();
    // Clear all has already emptied the filter through its facet-reset
    // handler. SectionSearch still applies the empty query afterward so its
    // commit half can remove the chip; that apply is model-neutral.
    if previous == text {
        return;
    }
    prepare_filter_change(shared, previous.as_str(), text);
    *shared.filter.borrow_mut() = text.to_string();
    reload_filter_change(shared, previous.as_str());
}

/// Captures the place a search leaves before its query is stored. The window
/// calls this synchronously because browser state must follow the entry while
/// the reload itself remains debounced.
pub(in crate::ui) fn prepare_filter_change(shared: &Rc<Shared>, previous: &str, current: &str) {
    // SEARCH-9: the empty → non-empty transition is the moment the user leaves
    // their place. Capture it once; a refinement of an existing query must not
    // overwrite it with a position inside the result set.
    if previous.is_empty() && !current.is_empty() {
        let captured = capture_reload_anchor(shared);
        shared.pre_search.set(super::PreSearch {
            anchor: captured.anchor,
            playback_started: false,
        });
    }
}

/// Reloads a filter already stored in `Shared`, retaining the previous value
/// solely to choose SEARCH-9's viewport behavior.
pub(in crate::ui) fn reload_filter_change(shared: &Rc<Shared>, previous: &str) {
    let current = shared.filter.borrow().clone();
    let started_in_search = shared.pre_search.get().playback_started;
    let viewport = filter_change_viewport(previous, current.as_str(), started_in_search);
    reload_with_viewport(shared, viewport);
}

/// Re-runs the current query while centering the loaded track when it remains
/// visible. Browse-facet and AI-filter callbacks use this because their filter
/// state is owned by `BrowseBar`, not by the search string above.
pub(in crate::ui) fn reload_centering_playing_track(shared: &Rc<Shared>) {
    reload_with_viewport(shared, ReloadViewport::CenterPlayingTrack);
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
pub(in crate::ui) fn set_source_and_reload(shared: &Rc<Shared>, source: &ViewSource) {
    let old_source = source_snapshot(&shared.source);
    if old_source == *source {
        reload(shared);
        return;
    }
    *shared.filter.borrow_mut() = String::new();
    // SEARCH-9: an anchor from the previous source points at a row this view
    // does not contain.
    shared.pre_search.set(super::PreSearch::default());
    *shared.browse_filter.borrow_mut() = BrowseFilter::default();
    shared.browse_bar.restore_filter(&BrowseFilter::default());
    let new_sort = resolve_sort_on_switch(&Default::default(), source);
    *shared.sort.borrow_mut() = new_sort;
    *shared.source.borrow_mut() = source.clone();
    shared.browse_bar.set_source_context(source);
    shared.selection.unselect_all();
    if let Some(adjustment) = gtk4::prelude::ScrollableExt::vadjustment(&shared.column_view) {
        adjustment.set_value(0.0);
    }
    if let Some(callback) = shared.on_search_restored.borrow().as_ref() {
        callback("");
    }
    run_query_if_requested(shared, None);
}

/// Re-runs the query against the current source/sort/filter state via
/// `TrackListModel::set_query`, then restores the pre-swap selection/scroll
/// anchor (TAG-1) — see this module's doc comment. Every caller except
/// `set_source_and_reload`'s source-switch branch goes through here.
pub(in crate::ui) fn reload(shared: &Rc<Shared>) {
    reload_with_viewport(shared, ReloadViewport::PreserveAnchor);
}

pub(in crate::ui) fn reload_with_viewport(shared: &Rc<Shared>, viewport: ReloadViewport) {
    let captured = capture_reload_anchor(shared);
    reload_with_anchor_and_viewport(shared, &captured, viewport, None, None);
}

/// Re-runs the current query while restoring a snapshot captured before an
/// asynchronous interaction began. The Tag Editor uses this seam because
/// capturing only when its worker finishes is too late: the closing dialog
/// and focus restoration may already have disturbed GTK's live adjustment.
pub(in crate::ui) fn reload_with_anchor(shared: &Rc<Shared>, captured: &ReloadAnchor) {
    reload_with_anchor_and_viewport(shared, captured, ReloadViewport::PreserveAnchor, None, None);
}

pub(in crate::ui) fn reload_with_anchor_and_viewport(
    shared: &Rc<Shared>,
    captured: &ReloadAnchor,
    viewport: ReloadViewport,
    model_change: Option<ModelChange>,
    current_ids: Option<Vec<i64>>,
) {
    if !shared.startup_load.request() {
        return;
    }
    // SEARCH-9: `Top` writes the adjustment itself and wants no guard fighting
    // it; only variants that may restore a captured position need one.
    let hold = matches!(
        viewport,
        ReloadViewport::PreserveAnchor
            | ReloadViewport::RestorePreSearch
            | ReloadViewport::CenterPlayingElsePreSearch
    )
    .then(|| gtk4::prelude::ScrollableExt::vadjustment(&shared.column_view))
    .flatten()
    .filter(|_| captured.anchor.is_some() || shared.pre_search.get().anchor.is_some())
    // A zero in a view the reload is about to leave is not a position worth
    // protecting: the hold would pin the list to the top while anchor restore
    // writes the meaningful destination.
    .filter(|adjustment| adjustment.value() > 0.0)
    .map(|adjustment| AdjustmentHold::new(&adjustment));
    run_query(shared, model_change);
    restore_reload_anchor(shared, captured, viewport, hold.as_ref(), current_ids);
    if let Some(hold) = hold {
        hold.release_after(SCROLL_ADJUSTMENT_HOLD);
    }
}

/// Direct source switches update the desired source before asking the startup
/// gate whether the model query may run. Deferred startup therefore retains
/// the final source while still collapsing the query itself.
fn run_query_if_requested(shared: &Rc<Shared>, model_change: Option<ModelChange>) {
    if !shared.startup_load.request() {
        return;
    }
    run_query(shared, model_change);
}

/// The bare query/model-swap/empty-state work, with no selection/scroll
/// handling of its own — see `reload`'s and `set_source_and_reload`'s doc
/// comments for who wraps this and why.
fn run_query(shared: &Rc<Shared>, model_change: Option<ModelChange>) {
    let n_sections = shared.queue_sections.borrow().len();
    if let Some(adjustment) = gtk4::prelude::ScrollableExt::vadjustment(&shared.column_view) {
        ListGeometry::for_view(&shared.column_view).remember_if_settled(
            &shared.conn,
            &shared.list_geometry_cache,
            adjustment.upper(),
            shared.model.n_items() as usize,
            n_sections,
        );
    }
    let sort = shared.sort.borrow().clone();
    let filter = shared.filter.borrow().clone();
    let source = shared.source.borrow().clone();
    let browse = if matches!(
        source,
        ViewSource::Library
            | ViewSource::Album { .. }
            | ViewSource::Artist(_)
            | ViewSource::Genre(_)
    ) {
        shared.browse_filter.borrow().clone()
    } else {
        BrowseFilter::default()
    };
    // FIL-7: the AI-exclude filter is opt-in and Library-only. Its sticky
    // state lives in the browse bar.
    let exclude_ai = shared.browse_bar.exclude_ai() && matches!(source, ViewSource::Library);
    let has_filter = !filter.trim().is_empty() || !browse.is_empty() || exclude_ai;

    let is_queue = matches!(source, ViewSource::Queue);
    let queue_context_window = is_queue.then(|| {
        let player = shared.player.borrow().clone();
        Rc::new(QueueContextWindow::from_player(player))
    });
    let queue_model = if is_queue {
        let queue_model = (shared.queue_ids_provider)();
        *shared.queue_sections.borrow_mut() = queue_model.sections.clone();
        Some(queue_model)
    } else {
        shared.queue_sections.borrow_mut().clear();
        None
    };
    if let (Some(queue_model), Some(context_window)) = (&queue_model, queue_context_window) {
        shared.model.set_queue_snapshot(
            queue_model,
            context_window,
            super::queue_sections::section_ranges(&queue_model.sections),
        );
    } else {
        shared.model.set_sections(Vec::new());
        match model_change {
            Some(change) => shared.model.set_query_browsed_ai_changed(
                &source,
                &sort.field,
                &sort.dir,
                &filter,
                &browse,
                &[],
                exclude_ai,
                change,
            ),
            None => shared.model.set_query_browsed_ai(
                &source,
                &sort.field,
                &sort.dir,
                &filter,
                &browse,
                &[],
                exclude_ai,
            ),
        }
    }

    // Strictly AFTER the query swap: installing a header factory flips
    // GTK's has_sections, which runs gtk_list_item_manager_ensure_items
    // SYNCHRONOUSLY — it must only ever see a model whose row count already
    // matches the section ranges declared above. Flipping it between
    // `set_sections` and `set_query_browsed` (the old order here) let GTK
    // read the new (small) Queue ranges against the old (large) row count;
    // with the viewport scrolled past the ranges' end that aborted the app
    // on the `header->widget == NULL` assertion in gtklistitemmanager.c.
    super::queue_sections::apply_queue_header_factory(shared, is_queue);
    if is_queue {
        super::track_list_geometry::schedule_section_measurement(shared);
    }

    // Stage 3 Task 8: the ImportErrors source's rows live in `import_errors_
    // view`, not `shared.model` (which `queries.rs` always resolves to an
    // empty window/count for this source — see its module doc's `ImportErrors`
    // section) — so its row count comes from refreshing that panel instead.
    let count = match source {
        ViewSource::ImportErrors => shared.import_errors_view.refresh(),
        ViewSource::Missing => shared.missing_files_view.refresh(),
        _ => shared.model.n_items() as usize,
    };
    browse_filter_count::update(
        &shared.browse_bar,
        &shared.conn,
        &source,
        count,
        &filter,
        &browse,
        exclude_ai,
        &[],
    );
    apply_empty_state(
        shared,
        empty_state_for_availability(
            count,
            has_filter,
            &source,
            shared.library_root_unavailable.get(),
        ),
    );
    shared
        .diagnostic_trail
        .record(super::diagnostic_trail::Event::Reload {
            source: source.label(),
            count,
        });
    shared
        .diagnostic_trail
        .record(super::diagnostic_trail::Event::StackPage {
            page: shared
                .stack
                .visible_child_name()
                .map_or_else(|| "none".into(), |page| page.to_string()),
        });

    crate::ui::startup_report::event("track_list_reload");
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
#[path = "track_list_reload_display_tests.rs"]
mod display_tests;

#[cfg(test)]
#[path = "fresh_start_allocation_display_tests.rs"]
mod fresh_start_allocation_display_tests;

#[cfg(test)]
#[path = "track_list_reload_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "reveal_track_display_tests.rs"]
mod reveal_track_display_tests;

#[cfg(test)]
#[path = "glide_reload_display_tests.rs"]
mod glide_reload_display_tests;

#[cfg(test)]
#[path = "search_viewport_display_tests.rs"]
mod search_viewport_display_tests;

#[cfg(test)]
#[path = "navback_anchor_display_tests.rs"]
mod navback_anchor_display_tests;

#[cfg(test)]
#[path = "queue_section_geometry_display_tests.rs"]
pub(in crate::ui::track_list) mod queue_section_geometry_display_tests;
