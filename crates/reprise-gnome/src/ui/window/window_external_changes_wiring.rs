//! Live refresh for writes this process did not make.

use std::path::Path;
use std::rc::Rc;

use crate::ui::sidebar::Sidebar;
use crate::ui::track_list::TrackList;

/// Wires the external-changes live refresh (multi-frontend-core package C):
/// mutations written to the same database by another process (CLI/MCP) reach
/// the running app through the change log and `events::Notifier`. The app's own
/// writes are filtered by its process writer token — it already refreshes
/// itself — so only foreign writes drive a coarse, silent refresh of the
/// sidebar and the current track list (UX rules EXT-1a..EXT-4). A notifier that
/// cannot start just means no live updates; it is never fatal.
pub(super) fn start_external_changes_refresh(
    db_path: &Path,
    track_list: &Rc<TrackList>,
    sidebar: &Rc<Sidebar>,
) {
    let sidebar = Rc::downgrade(sidebar);
    let track_list = Rc::downgrade(track_list);
    crate::ui::external_changes::start(
        db_path,
        Some(reprise_core::events::writer_token()),
        Rc::new(move |plan: crate::ui::external_changes::RefreshPlan| {
            if plan.sidebar {
                match sidebar.upgrade() {
                    Some(sidebar) => sidebar.refresh("external change"),
                    None => {
                        tracing::warn!("external change: sidebar refresh skipped: sidebar is gone");
                    }
                }
            }
            if plan.track_list {
                match track_list.upgrade() {
                    Some(track_list) => track_list.reload(),
                    None => {
                        tracing::warn!(
                            "external change: track list reload skipped: track list is gone"
                        );
                    }
                }
            }
        }),
    );
}
