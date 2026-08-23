//! Batch hide/restore behavior for Releases rows.

use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::artist_news;

use super::releases_view::{notify_refreshed, render_cache, Shared};
use crate::ui::strings;

const UNDO_TOAST_TIMEOUT_S: u32 = 10;

pub(super) fn set_hidden_batch(shared: &Rc<Shared>, mbids: Vec<String>, hidden: bool) {
    if mbids.is_empty() {
        return;
    }
    let positions = mbids
        .iter()
        .filter_map(|mbid| shared.model.position_of(mbid))
        .collect::<Vec<_>>();

    if !write_and_reload(shared, &mbids, hidden) {
        return;
    }

    let remaining = shared.model.store().n_items();
    if let Some(cursor) = selection_after_hide(&positions, remaining) {
        shared.model.selection().select_range(cursor, 1, true);
    } else {
        shared.model.selection().unselect_all();
    }

    show_undo_toast(shared, mbids, hidden);
}

fn write_and_reload(shared: &Rc<Shared>, mbids: &[String], hidden: bool) -> bool {
    if let Err(error) = artist_news::set_releases_hidden(&shared.conn, mbids, hidden) {
        tracing::warn!(%error, count = mbids.len(), "could not change release visibility");
        return false;
    }
    if let Err(error) = render_cache(shared) {
        tracing::warn!(%error, "could not reload Releases after visibility change");
    }
    notify_refreshed(shared);
    true
}

fn show_undo_toast(shared: &Rc<Shared>, mbids: Vec<String>, hidden: bool) -> Option<adw::Toast> {
    let Some(overlay) = shared.toast_overlay.upgrade() else {
        tracing::warn!("release visibility changed without an available toast overlay");
        return None;
    };
    let text = if hidden {
        strings::releases_hidden_toast(mbids.len())
    } else {
        strings::releases_restored_toast(mbids.len())
    };
    let shared = Rc::downgrade(shared);
    Some(crate::ui::toasts::show_with_action(
        &overlay,
        &text,
        &strings::text(strings::UNDO),
        UNDO_TOAST_TIMEOUT_S,
        move || {
            let Some(shared) = shared.upgrade() else {
                return;
            };
            if write_and_reload(&shared, &mbids, !hidden) {
                shared.model.select_mbids(&mbids);
            }
        },
    ))
}

/// The row that took the place of the first row that left, else the new last
/// row, else nothing -- a selection pointing at departed rows is not a state
/// this table is allowed to sit in.
pub(super) fn selection_after_hide(hidden_positions: &[u32], remaining: u32) -> Option<u32> {
    if remaining == 0 {
        return None;
    }
    let first = hidden_positions.iter().copied().min().unwrap_or(0);
    Some(first.min(remaining - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cursor_lands_on_the_row_that_moved_up() {
        assert_eq!(selection_after_hide(&[1, 2], 4), Some(1));
    }

    #[test]
    fn hiding_the_tail_falls_back_to_the_new_last_row() {
        assert_eq!(selection_after_hide(&[3, 4], 3), Some(2));
    }

    #[test]
    fn an_emptied_list_selects_nothing() {
        assert_eq!(selection_after_hide(&[0, 1], 0), None);
    }
}
