//! Shared UI invalidation after successful tag writes.

use std::path::PathBuf;
use std::rc::Rc;

use super::reload_restore::ReloadAnchor;
use super::track_list_reload::{capture_reload_anchor, reload_with_anchor};
use super::Shared;

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
        gtk4::glib::idle_add_local_once(move || reload_with_anchor(&shared, &anchor));
    }
    let callback = shared.on_tags_mutated.borrow().clone();
    if let Some(callback) = callback {
        callback(paths);
    }
}

#[cfg(test)]
#[path = "tag_mutation_refresh_display_tests.rs"]
mod display_tests;
