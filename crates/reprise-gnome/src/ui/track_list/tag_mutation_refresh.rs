//! Shared UI invalidation after successful tag writes.

use std::path::PathBuf;
use std::rc::Rc;

use super::track_list_reload::reload;
use super::Shared;

pub(in crate::ui) fn refresh_after_tag_mutation(
    shared: &Rc<Shared>,
    ids: &[i64],
    paths: &[PathBuf],
) {
    shared.cover_loader.invalidate_paths(paths);
    shared.browse_bar.refresh();
    if let Some(player) = shared.player.borrow().upgrade() {
        player.refresh_edited_metadata(ids);
    }
    reload(shared);
    let callback = shared.on_tags_mutated.borrow().clone();
    if let Some(callback) = callback {
        callback(paths);
    }
}
