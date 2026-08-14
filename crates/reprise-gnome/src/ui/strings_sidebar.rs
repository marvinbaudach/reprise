use super::formatted;

// Sidebar (src/ui/sidebar.rs, Stage 3 Task 4): navigation section headers,
// row labels, and the "New playlist" dialog. Section headers are given in
// the design mockup's all-caps form directly (not upper-cased at render
// time) since that's the exact copy the mockup shows, not a text-transform.

pub const SIDEBAR_SECTION_LIBRARY: &str = N_!("LIBRARY");
pub const SIDEBAR_SECTION_PLAYLISTS: &str = N_!("PLAYLISTS");
pub const SIDEBAR_SECTION_SMART: &str = N_!("SMART");
pub const SIDEBAR_SECTION_ISSUES: &str = N_!("ISSUES");

pub const SIDEBAR_MUSIC: &str = N_!("Music");
pub const SIDEBAR_RECENTLY_ADDED: &str = N_!("Recently added");
pub const SIDEBAR_QUEUE: &str = N_!("Queue");
pub const JUMP_TO_NOW_PLAYING: &str = N_!("Jump to now playing");
pub const GO_TO_PLAYING_ARTIST: &str = N_!("Go to playing artist");
pub const GO_TO_PLAYING_ALBUM: &str = N_!("Go to playing album");
pub const REVEAL_PLAYING_TRACK: &str = N_!("Reveal playing track");
pub const GO_TO_ALBUM_NAMED: &str = N_!("Go to album {album}");
pub const NAVIGATE_BACK: &str = N_!("Back to previous view");
pub const NAVIGATE_FORWARD: &str = N_!("Forward to next view");
pub const CONTEXT_MENU_PLAY_NEXT: &str = N_!("Play next");
pub const QUEUE_CLEAR_PLAY_NEXT: &str = N_!("Clear");
/// `{}` is the playback origin's display label (playlist/album/artist name
/// or the localized "Music").
pub const EMPTY_QUEUE_TITLE: &str = N_!("Nothing queued");
pub const EMPTY_QUEUE_DESCRIPTION: &str = N_!("Play something");
pub const SIDEBAR_NEW_PLAYLIST: &str = N_!("New playlist");
pub const SIDEBAR_IMPORT_ERRORS: &str = N_!("Import errors");
pub const SIDEBAR_MISSING_FILES: &str = N_!("Missing files");
pub const SIDEBAR_MY_STATS: &str = N_!("My Stats");

/// Tooltip/accessible name for the headerbar's persistent sidebar-visibility
/// toggle.
pub const SIDEBAR_TOGGLE: &str = N_!("Toggle sidebar");

pub const NEW_PLAYLIST_DIALOG_HEADING: &str = N_!("New playlist");
pub const NEW_PLAYLIST_ENTRY_PLACEHOLDER: &str = N_!("Playlist name");
pub const CANCEL: &str = N_!("Cancel");
pub const CREATE: &str = N_!("Create");

/// Toast shown when `library::playlists::create` fails while handling the
/// sidebar's "New playlist" dialog.
pub fn playlist_create_failed_toast(name: &str) -> String {
    formatted(N_!("Could not create playlist “{name}”"), &[("name", name)])
}
