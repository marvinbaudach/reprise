use std::rc::Rc;

use reprise_core::library::playlists;
use reprise_core::view_source::ViewSource;

use crate::ui::dialogs;
use crate::ui::sidebar::{rebuild, show_toast, Shared};
use crate::ui::strings;

/// An empty playlist is only a new destination. Refresh the sidebar while
/// preserving its current source so the user can keep selecting tracks to
/// fill it.
pub(in crate::ui) fn refresh_target_after_empty_creation() -> Option<ViewSource> {
    None
}

/// Shows the "New playlist" `AdwAlertDialog`: a heading, an entry (Create
/// disabled until non-blank), and Cancel/Create responses. On Create, it
/// creates the playlist while keeping the current source visible.
pub(in crate::ui) fn show_new_playlist_dialog(shared: &Rc<Shared>) {
    let Some(window) = shared.window.upgrade() else {
        tracing::warn!("sidebar: window is gone; cannot show new-playlist dialog");
        return;
    };

    let shared = shared.clone();
    dialogs::prompt_name(
        &window,
        &strings::text(strings::NEW_PLAYLIST_DIALOG_HEADING),
        &strings::text(strings::NEW_PLAYLIST_ENTRY_PLACEHOLDER),
        &strings::text(strings::CREATE),
        move |name| create_playlist_and_stay(&shared, &name),
    );
}

/// Creates a playlist named `name` and refreshes the sidebar without leaving
/// the current source. A creation failure is logged and surfaced as a toast.
fn create_playlist_and_stay(shared: &Rc<Shared>, name: &str) {
    let created = {
        let conn = &shared.conn;
        playlists::create(conn, name)
    };
    match created {
        Ok(id) => {
            tracing::info!(id, name, "playlist created");
            rebuild(
                shared,
                refresh_target_after_empty_creation(),
                "playlist created",
            );
        }
        Err(error) => {
            tracing::error!(%error, name, "failed to create playlist");
            show_toast(shared, &strings::playlist_create_failed_toast(name));
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
