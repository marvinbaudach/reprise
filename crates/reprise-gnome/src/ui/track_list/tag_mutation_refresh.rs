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
    reload_with_anchor(shared, &anchor);
    // The tag editor is a dialog whose save completes on the main loop just as
    // the dialog is animating shut, so the `ColumnView` behind it can still be
    // obscured / not yet re-mapped when this first `reload`'s `items_changed`
    // fires — and GTK then skips rebinding the not-yet-visible rows, leaving the
    // live view showing the PRE-EDIT tags until the next manual reload (a header
    // click / new search). Schedule one more reload on idle, after the dialog
    // has closed and the table is mapped again, so the edited rows are
    // guaranteed to rebind. Cheap (a single re-query) and idempotent.
    {
        let shared = shared.clone();
        gtk4::glib::idle_add_local_once(move || reload_with_anchor(&shared, &anchor));
    }
    let callback = shared.on_tags_mutated.borrow().clone();
    if let Some(callback) = callback {
        callback(paths);
    }
}
