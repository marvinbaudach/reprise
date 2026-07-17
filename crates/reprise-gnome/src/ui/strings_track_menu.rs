//! Labels owned by the shared track-row context menu.

use super::{plural, text, MOVE_TO_TRASH, REMOVE_FROM_LIBRARY};

pub const CONTEXT_MENU_PLAY: &str = N_!("Play");
pub const CONTEXT_MENU_ADD_TO_QUEUE: &str = N_!("Add to queue");
pub const CONTEXT_MENU_ADD_TO_PLAYLIST: &str = N_!("Add to playlist");
#[allow(dead_code)] // Used by the live track-menu adapter in Task 7.
pub const CONTEXT_MENU_MOVE_TO_TOP: &str = N_!("Move to top");
#[allow(dead_code)] // Used by the live track-menu adapter in Task 7.
pub const CONTEXT_MENU_GO_TO_ALBUM: &str = N_!("Go to album");
#[allow(dead_code)] // Used by the live track-menu adapter in Task 7.
pub const CONTEXT_MENU_GO_TO_ARTIST: &str = N_!("Go to artist");
#[allow(dead_code)] // Used by the live track-menu adapter in Task 7.
pub const CONTEXT_MENU_SHOW_IN_FILES: &str = N_!("Show in Files");
#[allow(dead_code)] // Used by the live track-menu adapter in Task 7.
pub const CONTEXT_MENU_SHOW_IN_MISSING: &str = N_!("Show in Missing files");
/// Leaf at the bottom of the submenu. The ellipsis denotes opening a dialog.
pub const CONTEXT_MENU_NEW_PLAYLIST: &str = N_!("New playlist…");
pub const CONTEXT_MENU_REMOVE_FROM_PLAYLIST: &str = N_!("Remove from playlist");

#[allow(dead_code)] // Used by the live track-menu adapter in Task 7.
pub fn remove_from_playlist_label(count: usize) -> String {
    destructive_count_label(
        count,
        CONTEXT_MENU_REMOVE_FROM_PLAYLIST,
        N_!("Remove {count} from playlist"),
    )
}

#[allow(dead_code)] // Used by the live track-menu adapter in Task 7.
pub fn remove_from_queue_label(count: usize) -> String {
    destructive_count_label(
        count,
        N_!("Remove from queue"),
        N_!("Remove {count} from queue"),
    )
}

#[allow(dead_code)] // Used by the live track-menu adapter in Task 7.
pub fn remove_from_library_label(count: usize) -> String {
    destructive_count_label(
        count,
        REMOVE_FROM_LIBRARY,
        N_!("Remove {count} from library…"),
    )
}

#[allow(dead_code)] // Used by the live track-menu adapter in Task 7.
pub fn move_to_trash_label(count: usize) -> String {
    destructive_count_label(count, MOVE_TO_TRASH, N_!("Move {count} to Trash…"))
}

#[allow(dead_code)] // Used by the live track-menu adapter in Task 7.
fn destructive_count_label(count: usize, singular: &str, plural_message: &str) -> String {
    if count <= 1 {
        return text(singular);
    }
    let count_text = count.to_string();
    plural(
        singular,
        plural_message,
        count,
        &[("count", count_text.as_str())],
    )
}
