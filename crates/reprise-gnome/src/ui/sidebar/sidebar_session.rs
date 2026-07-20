//! Session-specific sidebar selection with the standard vanished-row fallback.

use std::rc::Rc;

use reprise_core::view_source::ViewSource;

use crate::ui::sidebar::{find_row, resolve_select_source, select_row_in_its_listbox, Shared};
use crate::ui::strings;
use crate::ui::toasts;

pub(in crate::ui) fn restore_source(
    shared: &Rc<Shared>,
    requested: ViewSource,
) -> (ViewSource, String) {
    let row_exists = find_row(shared, &requested).is_some();
    let source = resolve_select_source(requested, row_exists).0;
    let entry = shared
        .rows
        .borrow()
        .iter()
        .find(|(_, candidate, _)| candidate == &source)
        .map(|(row, _, title)| (row.clone(), title.clone()));
    let Some((row, title)) = entry else {
        return (ViewSource::Library, strings::text(strings::SIDEBAR_MUSIC));
    };
    *shared.current_source.borrow_mut() = source.clone();
    select_row_in_its_listbox(&row);
    (source, title)
}

/// Re-baselines the row-selected dedup against the view's ACTUAL source.
/// Paths that change the track list without going through the sidebar
/// (album/artist cross-navigation, smoke hooks) leave `current_source`
/// stale; NAV-9b's jump and NAV-2's back call this first so their
/// `refresh_and_select` isn't swallowed as a same-source no-op.
pub(in crate::ui) fn sync_current_source(
    shared: &super::sidebar::Shared,
    source: &reprise_core::view_source::ViewSource,
) {
    *shared.current_source.borrow_mut() = source.clone();
}

fn reroute_baseline(target: &ViewSource) -> ViewSource {
    if matches!(target, ViewSource::MyStats) {
        ViewSource::Library
    } else {
        ViewSource::MyStats
    }
}

/// Forces the next rebuilt row selection through the routing callback.
///
/// A detail surface such as My Stats can leave the track list's source at
/// Library. Re-baselining from that stale source would deduplicate a history
/// return to Library and leave the detail page visible. A deliberately
/// different sentinel makes the rebuilt target row authoritative again.
pub(in crate::ui) fn prepare_history_reroute(shared: &Shared, target: &ViewSource) {
    sync_current_source(shared, &reroute_baseline(target));
}

// Queue-nav-row drop seams, relocated from `sidebar.rs` (orchestrator size
// rule) — same overflow home as `sync_current_source` above.
impl crate::ui::sidebar::Sidebar {
    /// Sets the callback invoked when tracks are dropped onto the Queue nav
    /// row — see `Shared::on_queue_drop`'s doc comment. `window.rs` wires
    /// this to `PlayerController::append_to_queue`.
    pub fn set_on_queue_drop(&self, callback: impl Fn(&[i64]) -> bool + 'static) {
        *self.shared.on_queue_drop.borrow_mut() = Some(Rc::new(callback));
    }

    /// Drives the same drop-handling sequence `sidebar_dnd::wire_queue_drop_
    /// target`'s real `connect_drop` closure runs, for callers that can't
    /// synthesize a pointer drag — the Queue-row analogue of [`Self::handle_
    /// playlist_drop`], reached the same way (`window.rs` wires it to
    /// `TrackList::set_on_sidebar_queue_drop` for `ui::track_list_dnd_smoke`'s
    /// `REPRISE_SMOKE_DND=addqueue` hook). Returns whether anything was
    /// actually appended.
    pub fn handle_queue_drop(&self, ids: &[i64]) -> bool {
        super::sidebar_dnd::handle_queue_drop(&self.shared, ids)
    }
}

/// Shows `text` as an `adw::Toast`, degrading to a warn log if no overlay is
/// wired or it's gone — mirrors `track_list.rs`/`player_controller.rs`'s
/// `show_toast` (same seam, same degrade behavior).
pub(in crate::ui) fn show_toast(shared: &super::sidebar::Shared, text: &str) {
    match shared.toast_overlay.upgrade() {
        Some(overlay) => toasts::show(&overlay, text),
        None => {
            tracing::warn!(text, "toast overlay is gone; degrading to log-only");
        }
    }
}

impl crate::ui::sidebar::Sidebar {
    #[cfg(test)]
    pub(in crate::ui) fn test_shared(&self) -> &std::rc::Rc<super::sidebar::Shared> {
        &self.shared
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acc_5_history_reroute_cannot_be_deduplicated_as_the_target_source() {
        for target in [
            ViewSource::Library,
            ViewSource::Queue,
            ViewSource::Missing,
            ViewSource::MyStats,
        ] {
            assert_ne!(reroute_baseline(&target), target);
        }
    }
}
