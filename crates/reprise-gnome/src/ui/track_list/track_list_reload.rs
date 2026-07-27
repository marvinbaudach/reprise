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

use gtk4::gio::prelude::*;
use gtk4::prelude::*;

use crate::ui::browse_filter_count;
use crate::ui::track_list::reload_restore::{self, ReloadAnchor};
use crate::ui::track_list::track_list_empty_state::{
    apply_empty_state, empty_state_for_availability,
};
use crate::ui::track_list::Shared;
use crate::ui::track_list_sort::resolve_sort_on_switch;
use reprise_core::queries::BrowseFilter;
use reprise_core::view_source::ViewSource;

/// How many idle-callback rounds the scroll restore waits for the rebuilt
/// list to gain usable geometry before giving up — mirrors `view_state_
/// memory`'s identical constant (BROWSE-2), which faces the same "freshly
/// repopulated `ColumnView` doesn't have adjustment geometry until the next
/// allocation pass" issue.
const SCROLL_RESTORE_MAX_ATTEMPTS: u8 = 8;

#[derive(Clone, Copy)]
enum ReloadViewport {
    PreserveAnchor,
    CenterPlayingTrack,
}

fn filter_change_viewport(previous: &str, current: &str) -> ReloadViewport {
    if previous == current {
        ReloadViewport::PreserveAnchor
    } else {
        ReloadViewport::CenterPlayingTrack
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

/// Approximates the uniform row height from the adjustment's total content
/// height over the row count — the same technique `current_track_selection::
/// centered_scroll_value` uses for the "jump to now playing" center (NAV-9b):
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
pub(in crate::ui) fn capture_reload_anchor(shared: &Shared) -> ReloadAnchor {
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
fn restore_reload_anchor(shared: &Shared, captured: &ReloadAnchor, viewport: ReloadViewport) {
    // Resolving positions costs a sorted full-table id query; skip it when
    // the capture side already established there is nothing to put back and
    // the caller did not request a playing-track reveal.
    let reveal_playing_track = matches!(viewport, ReloadViewport::CenterPlayingTrack)
        && shared.playing_track_id.get().is_some();
    if reload_restore::is_noop(captured) && !reveal_playing_track {
        return;
    }
    let current_ids = shared.current_view_ids();
    if !reload_restore::is_noop(captured) {
        let positions = reload_restore::positions_for_ids(&captured.selected_ids, &current_ids);
        shared.selection.unselect_all();
        for position in positions {
            shared.selection.select_item(position, false);
        }
    }

    if matches!(viewport, ReloadViewport::CenterPlayingTrack) {
        let playing_track_id = shared.playing_track_id.get();
        if playing_track_id.is_some_and(|track_id| current_ids.contains(&track_id)) {
            schedule_centered_scroll_restore(
                shared.column_view.clone(),
                playing_track_id,
                current_ids,
                SCROLL_RESTORE_MAX_ATTEMPTS,
            );
            return;
        }
    }
    schedule_scroll_restore(
        shared.column_view.clone(),
        captured.anchor,
        current_ids,
        SCROLL_RESTORE_MAX_ATTEMPTS,
    );
}

fn schedule_centered_scroll_restore(
    column_view: gtk4::ColumnView,
    track_id: Option<i64>,
    current_ids: Vec<i64>,
    attempts: u8,
) {
    let anchor = track_id.map(|track_id| (track_id, 0.0));
    let Some(position) = reload_restore::prepaint_position(anchor, &current_ids) else {
        return;
    };
    let scroll = gtk4::ScrollInfo::new();
    scroll.set_enable_vertical(true);
    column_view.scroll_to(position, None, gtk4::ListScrollFlags::NONE, Some(scroll));
    gtk4::glib::idle_add_local_once(move || {
        let Some(adjustment) = gtk4::prelude::ScrollableExt::vadjustment(&column_view) else {
            return;
        };
        let (upper, page) = (adjustment.upper(), adjustment.page_size());
        if upper > page {
            let height = upper / current_ids.len() as f64;
            if let Some(value) =
                reload_restore::centered_track_scroll_target(track_id, &current_ids, height, page)
            {
                adjustment.set_value(value);
            }
        } else if attempts > 0 {
            schedule_centered_scroll_restore(column_view, track_id, current_ids, attempts - 1);
        }
    });
}

fn schedule_scroll_restore(
    column_view: gtk4::ColumnView,
    anchor: Option<(i64, f64)>,
    current_ids: Vec<i64>,
    attempts: u8,
) {
    let Some(position) = reload_restore::prepaint_position(anchor, &current_ids) else {
        return;
    };
    // `items_changed(0, old, new)` resets GtkColumnView's adjustment to zero
    // synchronously. Queue a stable-id scroll before returning to the main
    // loop, so GTK never paints that transient top-of-table state. This API
    // also works while the tag dialog is still closing or the table is not
    // mapped yet. The idle retry below refines the result to the captured
    // within-row pixel offset once the rebuilt list has usable geometry.
    let scroll = gtk4::ScrollInfo::new();
    scroll.set_enable_vertical(true);
    column_view.scroll_to(position, None, gtk4::ListScrollFlags::NONE, Some(scroll));
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
    let viewport = filter_change_viewport(shared.filter.borrow().as_str(), text);
    *shared.filter.borrow_mut() = text.to_string();
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
    run_query(shared);
}

/// Re-runs the query against the current source/sort/filter state via
/// `TrackListModel::set_query`, then restores the pre-swap selection/scroll
/// anchor (TAG-1) — see this module's doc comment. Every caller except
/// `set_source_and_reload`'s source-switch branch goes through here.
pub(in crate::ui) fn reload(shared: &Rc<Shared>) {
    reload_with_viewport(shared, ReloadViewport::PreserveAnchor);
}

fn reload_with_viewport(shared: &Rc<Shared>, viewport: ReloadViewport) {
    let captured = capture_reload_anchor(shared);
    run_query(shared);
    restore_reload_anchor(shared, &captured, viewport);
}

/// Re-runs the current query while restoring a snapshot captured before an
/// asynchronous interaction began. The Tag Editor uses this seam because
/// capturing only when its worker finishes is too late: the closing dialog
/// and focus restoration may already have disturbed GTK's live adjustment.
pub(in crate::ui) fn reload_with_anchor(shared: &Rc<Shared>, captured: &ReloadAnchor) {
    run_query(shared);
    restore_reload_anchor(shared, captured, ReloadViewport::PreserveAnchor);
}

/// The bare query/model-swap/empty-state work, with no selection/scroll
/// handling of its own — see `reload`'s and `set_source_and_reload`'s doc
/// comments for who wraps this and why.
fn run_query(shared: &Rc<Shared>) {
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
    // FIL-7: the AI-exclude filter is opt-in, Library-only, and gated on the
    // experimental switch (INST-11). Its sticky state lives in the browse bar.
    let exclude_ai = shared.browse_bar.exclude_ai()
        && matches!(source, ViewSource::Library)
        && crate::ui::instrumental::experimental_enabled(&shared.conn.borrow());
    let has_filter = !filter.trim().is_empty() || !browse.is_empty() || exclude_ai;

    let is_queue = matches!(source, ViewSource::Queue);
    let queue_model = if is_queue {
        let queue_model = (shared.queue_ids_provider)();
        *shared.queue_sections.borrow_mut() = queue_model.sections.clone();
        Some(queue_model)
    } else {
        shared.queue_sections.borrow_mut().clear();
        None
    };

    if let Some(queue_model) = &queue_model {
        shared.model.set_queue_snapshot(
            queue_model,
            super::queue_sections::section_ranges(&queue_model.sections),
        );
    } else {
        shared.model.set_sections(Vec::new());
        shared.model.set_query_browsed_ai(
            &source,
            &sort.field,
            &sort.dir,
            &filter,
            &browse,
            &[],
            exclude_ai,
        );
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
mod display_tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn tag_1_query_reloading_metadata_save_keeps_the_live_viewport() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let mut conn = Connection::open_in_memory().unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        let tx = conn.transaction().unwrap();
        for id in 1..=100 {
            tx.execute(
                "INSERT INTO tracks (id, path, title, artist, added_at) \
                 VALUES (?1, ?2, ?3, 'Synthetic Artist', 0)",
                (
                    id,
                    format!("/synthetic/{id:03}.flac"),
                    format!("Track {id:03}"),
                ),
            )
            .unwrap();
        }
        tx.commit().unwrap();
        let track_list = super::super::TrackList::new(
            Rc::new(RefCell::new(conn)),
            Box::new(|_, _, _, _| {}),
            |_, _, _, _| {},
            super::super::queue_sections::QueueViewModel::default,
            crate::ui::cover_download_worker::setup_for_test(),
        );
        let window = gtk4::Window::builder()
            .default_width(900)
            .default_height(320)
            .child(track_list.widget())
            .build();
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        let position = 60;
        track_list
            .shared
            .column_view
            .scroll_to(position, None, gtk4::ListScrollFlags::FOCUS, None);
        let adjustment = track_list.shared.column_view.vadjustment().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while adjustment.value() <= 0.0 && std::time::Instant::now() < deadline {
            gtk4::glib::MainContext::default().iteration(false);
        }
        let before = adjustment.value();
        assert!(
            before > 0.0,
            "precondition: the list must be scrolled away from the top"
        );

        let opened_anchor = capture_reload_anchor(&track_list.shared);
        // Reproduce the asynchronous Tag Editor boundary: by the time the
        // worker completes, GTK may already report position zero while the
        // closing dialog restores focus. Capturing at completion would
        // therefore preserve the wrong position.
        adjustment.set_value(0.0);
        track_list.shared.selection.unselect_all();
        let written_id = track_list.shared.model.track_at(position).unwrap().id;
        let mut save_anchor = opened_anchor;
        save_anchor.selected_ids = vec![written_id];
        reload_with_anchor(&track_list.shared, &save_anchor);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        while (adjustment.value() - before).abs() >= 1.0 && std::time::Instant::now() < deadline {
            gtk4::glib::MainContext::default().iteration(false);
        }

        assert!(
            adjustment.value() > 0.0,
            "rating save must not leave the viewport at the table top"
        );
        assert!(
            (adjustment.value() - before).abs() < 1.0,
            "rating save moved the viewport: before={before}, after={}",
            adjustment.value()
        );
        assert!(track_list.shared.selection.is_selected(position));
        window.close();
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use reprise_core::view_source::ViewSource;

    use super::{filter_change_viewport, ReloadViewport};

    #[test]
    fn source_snapshot_releases_the_borrow_before_reentrant_work() {
        let source = RefCell::new(ViewSource::Library);

        let snapshot = super::source_snapshot(&source);
        *source.borrow_mut() = ViewSource::Queue;

        assert!(matches!(snapshot, ViewSource::Library));
        assert!(matches!(*source.borrow(), ViewSource::Queue));
    }

    #[test]
    fn fil_9_any_search_change_requests_playing_track_centering() {
        assert!(matches!(
            filter_change_viewport("", "Match"),
            ReloadViewport::CenterPlayingTrack
        ));
        assert!(matches!(
            filter_change_viewport("Match", ""),
            ReloadViewport::CenterPlayingTrack
        ));
        assert!(matches!(
            filter_change_viewport("Match", "Match"),
            ReloadViewport::PreserveAnchor
        ));
    }
}
