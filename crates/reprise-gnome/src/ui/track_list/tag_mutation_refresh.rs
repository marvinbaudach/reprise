//! Shared UI invalidation after successful tag writes.

use std::path::PathBuf;
use std::rc::Rc;

use reprise_core::queries::BrowseFilter;
use reprise_core::view_source::ViewSource;

use super::reload_restore::ReloadAnchor;
use super::track_list_model_change::{changed_range, ModelChange};
use super::track_list_reload::{
    capture_reload_anchor, reload_with_anchor, reload_with_anchor_and_viewport, ReloadViewport,
};
use super::Shared;

#[derive(Clone, PartialEq)]
struct ReloadQueryKey {
    source: ViewSource,
    sort_field: String,
    sort_dir: String,
    filter: String,
    browse: BrowseFilter,
    exclude_ai: bool,
}

struct ReloadChange {
    model: ModelChange,
    current_ids: Vec<i64>,
    query: ReloadQueryKey,
}

fn reload_query_key(shared: &Shared) -> ReloadQueryKey {
    let source = shared.source.borrow().clone();
    let sort = shared.sort.borrow().clone();
    ReloadQueryKey {
        exclude_ai: shared.browse_bar.exclude_ai() && matches!(source, ViewSource::Library),
        source,
        sort_field: sort.field,
        sort_dir: sort.dir,
        filter: shared.filter.borrow().clone(),
        browse: shared.browse_filter.borrow().clone(),
    }
}

pub(in crate::ui) fn refresh_after_tag_mutation(
    shared: &Rc<Shared>,
    ids: &[i64],
    paths: &[PathBuf],
) {
    let anchor = capture_reload_anchor(shared);
    refresh_after_tag_mutation_with_anchor(shared, ids, paths, anchor);
}

pub(in crate::ui) fn refresh_after_tag_mutation_with_anchor(
    shared: &Rc<Shared>,
    ids: &[i64],
    paths: &[PathBuf],
    anchor: ReloadAnchor,
) {
    refresh_with_reload_change(shared, ids, paths, anchor, None);
}

pub(in crate::ui) fn refresh_after_tag_mutation_with_view_ids(
    shared: &Rc<Shared>,
    ids: &[i64],
    paths: &[PathBuf],
    anchor: ReloadAnchor,
    before_ids: &[i64],
    after_ids: Vec<i64>,
) {
    let reload_change = changed_range(before_ids, &after_ids, ids).map(|model| ReloadChange {
        model,
        current_ids: after_ids,
        query: reload_query_key(shared),
    });
    refresh_with_reload_change(shared, ids, paths, anchor, reload_change);
}

fn refresh_with_reload_change(
    shared: &Rc<Shared>,
    ids: &[i64],
    paths: &[PathBuf],
    anchor: ReloadAnchor,
    reload_change: Option<ReloadChange>,
) {
    shared.cover_loader.invalidate_paths(paths);
    shared.browse_bar.refresh();
    if let Some(player) = shared.player.borrow().upgrade() {
        player.refresh_edited_metadata(ids);
    }
    // One reload, on idle. The tag editor is a dialog whose save completes on
    // the main loop just as the dialog is animating shut, so the `ColumnView`
    // behind it can still be obscured / not yet re-mapped: GTK then skips
    // rebinding the not-yet-visible rows and the live view keeps showing the
    // PRE-EDIT tags until the next manual reload (a header click / new
    // search). Deferring past the current main-loop turn is what makes the
    // edited rows rebind.
    //
    // This used to run *twice* — once synchronously here and once on idle —
    // and the synchronous one is the one that cannot be trusted to rebind. It
    // was not free either: every `reload` is a sorted full-table id query plus
    // an `items_changed(0, old, new)` that collapses selection and scroll for
    // `track_list_reload`'s restore to put back again.
    {
        let shared = shared.clone();
        gtk4::glib::idle_add_local_once(move || match reload_change {
            Some(change) if change.query == reload_query_key(&shared) => {
                reload_with_anchor_and_viewport(
                    &shared,
                    &anchor,
                    ReloadViewport::PreserveAnchor,
                    Some(change.model),
                    Some(change.current_ids),
                );
            }
            None => reload_with_anchor(&shared, &anchor),
            Some(_) => reload_with_anchor(&shared, &anchor),
        });
    }
    let callback = shared.on_tags_mutated.borrow().clone();
    if let Some(callback) = callback {
        callback(paths);
    }
}

#[cfg(test)]
#[path = "tag_mutation_refresh_display_tests.rs"]
mod display_tests;
