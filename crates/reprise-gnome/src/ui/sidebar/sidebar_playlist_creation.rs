use std::rc::Rc;

use reprise_core::library::playlists;
use reprise_core::view_source::ViewSource;

use crate::ui::sidebar::{rebuild, show_toast, Shared};
use crate::ui::strings;

/// An empty playlist is only a new destination. Refresh the sidebar while
/// preserving its current source so the user can keep selecting tracks to
/// fill it.
pub(in crate::ui) fn refresh_target_after_empty_creation() -> Option<ViewSource> {
    None
}

/// Creates a playlist named `name` and refreshes the sidebar without leaving
/// the current source. A creation failure is logged and surfaced as a toast.
pub(in crate::ui) fn create_playlist_and_stay(shared: &Rc<Shared>, name: &str) -> Option<i64> {
    let created = {
        let conn = &shared.conn;
        playlists::create(conn, name)
    };
    match created {
        Ok(id) => {
            tracing::info!(id, name, "playlist created");
            shared.playlist_quick_edit_id.set(Some(id));
            rebuild(
                shared,
                refresh_target_after_empty_creation(),
                "playlist created",
            );
            Some(id)
        }
        Err(error) => {
            tracing::error!(%error, name, "failed to create playlist");
            show_toast(shared, &strings::playlist_create_failed_toast(name));
            None
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn empty_playlist_creation_keeps_the_current_library_source() {
        assert_eq!(super::refresh_target_after_empty_creation(), None);
    }
}
