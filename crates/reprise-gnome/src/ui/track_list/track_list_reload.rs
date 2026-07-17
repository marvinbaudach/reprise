//! Query reload and source/filter transitions for TrackList.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gio::prelude::*;

use crate::ui::browse_filter_count;
use crate::ui::track_list::Shared;
use crate::ui::track_list_activation::current_queue_ids;
use crate::ui::track_list_columns::{apply_empty_state, empty_state_for};
use crate::ui::track_list_sort::resolve_sort_on_switch;
use reprise_core::queries::BrowseFilter;
use reprise_core::view_source::ViewSource;

fn source_snapshot(source: &RefCell<ViewSource>) -> ViewSource {
    source.borrow().clone()
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
    // after `reload` rebuilt it. Same-source calls are plain reloads and
    // deliberately skip both halves.
    let old_source = source_snapshot(&shared.source);
    super::view_state_memory::remember_on_leave(shared, &old_source, &source);
    // Hoisted so the `sort` borrow ends before the `borrow_mut` below.
    let new_sort = resolve_sort_on_switch(&shared.sort.borrow(), &source);
    *shared.sort.borrow_mut() = new_sort;
    *shared.source.borrow_mut() = source;
    let source = source_snapshot(&shared.source);
    shared
        .browse_bar
        .set_library_visible(matches!(source, ViewSource::Library));
    reload(shared);
    if old_source != source {
        let current_ids = shared.current_view_ids();
        super::view_state_memory::restore_on_attach(shared, &source, &current_ids);
    }
}

/// Re-runs the query against the current source/sort/filter state via
/// `TrackListModel::set_query`. Switches the stack to whichever page
/// `empty_state_for` selects for the resulting row count, filter state, and
/// source.
pub(in crate::ui) fn reload(shared: &Rc<Shared>) {
    let sort = shared.sort.borrow().clone();
    let filter = shared.filter.borrow().clone();
    let source = shared.source.borrow().clone();
    let browse = if matches!(source, ViewSource::Library) {
        shared.browse_filter.borrow().clone()
    } else {
        BrowseFilter::default()
    };
    let has_filter = !filter.trim().is_empty() || !browse.is_empty();

    let queue_ids = if matches!(source, ViewSource::Queue) {
        current_queue_ids(shared)
    } else {
        Vec::new()
    };

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
    browse_filter_count::update(&shared.browse_bar, &shared.conn, &source, count, has_filter);
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
