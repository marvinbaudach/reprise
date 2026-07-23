//! Labels owned by the shared track-row context menu.

use super::{plural, text, MOVE_TO_TRASH, REMOVE_FROM_LIBRARY};

pub const CONTEXT_MENU_ADD_TO_QUEUE: &str = N_!("Add to queue");
pub const CONTEXT_MENU_ADD_TO_PLAYLIST: &str = N_!("Add to playlist");
pub const CONTEXT_MENU_MOVE_TO_TOP: &str = N_!("Move to top");
pub const CONTEXT_MENU_GO_TO_ALBUM: &str = N_!("Go to album");
pub const CONTEXT_MENU_GO_TO_ARTIST: &str = N_!("Go to artist");
pub const CONTEXT_MENU_SHOW_IN_FILES: &str = N_!("Show in Files");
pub const CONTEXT_MENU_SHOW_IN_MISSING: &str = N_!("Show in Missing files");
/// Leaf at the bottom of the submenu. The ellipsis denotes opening a dialog.
pub const CONTEXT_MENU_NEW_PLAYLIST: &str = N_!("New playlist…");
pub const CONTEXT_MENU_REMOVE_FROM_PLAYLIST: &str = N_!("Remove from playlist");

pub fn remove_from_playlist_label(count: usize) -> String {
    destructive_count_label(
        count,
        CONTEXT_MENU_REMOVE_FROM_PLAYLIST,
        N_!("Remove {count} from playlist"),
    )
}

pub fn remove_from_queue_label(count: usize) -> String {
    destructive_count_label(
        count,
        N_!("Remove from queue"),
        N_!("Remove {count} from queue"),
    )
}

pub fn remove_from_library_label(count: usize) -> String {
    destructive_count_label(
        count,
        REMOVE_FROM_LIBRARY,
        N_!("Remove {count} from library…"),
    )
}

pub fn move_to_trash_label(count: usize) -> String {
    destructive_count_label(count, MOVE_TO_TRASH, N_!("Move {count} to Trash…"))
}

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

pub fn tracks_moved_to_top_toast(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        N_!("Moved {count} track to top"),
        N_!("Moved {count} tracks to top"),
        count,
        &[("count", count_text.as_str())],
    )
}
