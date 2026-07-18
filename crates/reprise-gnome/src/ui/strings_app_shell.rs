macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

pub const ALBUM_SORT_RECENTLY_ADDED: &str = N_!("Recently added");
pub const ALBUM_SORT_TITLE: &str = N_!("Title A–Z");
pub const ALBUM_SORT_ARTIST: &str = N_!("Artist A–Z");
pub const ALBUM_SORT_YEAR: &str = N_!("Year");
pub const ALBUM_SORT_MOST_PLAYED: &str = N_!("Most played");
pub const ALBUM_COUNT_FMT: &str = N_!("{} albums");
pub const ALBUM_SEARCH_EMPTY: &str = N_!("No albums match \"{}\"");

pub const ALBUM_MENU_PLAY: &str = N_!("Play");
pub const ALBUM_MENU_SHUFFLE: &str = N_!("Shuffle Album");
pub const ALBUM_MENU_ADD_QUEUE: &str = N_!("Add to Queue");
pub const ALBUM_MENU_ADD_PLAYLIST: &str = N_!("Add to Playlist");
pub const ALBUM_MENU_NEW_PLAYLIST: &str = N_!("New Playlist…");
pub const ALBUM_MENU_EDIT_TAGS: &str = N_!("Edit Tags");
pub const ALBUM_MENU_GO_TO_FOLDER: &str = N_!("Go to Folder");

/// Formats album duration: "1h 4min" or "42 min".
pub fn album_duration(total_ms: i64) -> String {
    let total_min = total_ms / 60_000;
    let hours = total_min / 60;
    let mins = total_min % 60;
    if hours > 0 {
        format!("{hours}h {mins}min")
    } else {
        format!("{mins} min")
    }
}

pub const ABOUT_REPRISE: &str = N_!("About Reprise");
pub const REPRISE_ENGINE_AND_LINUX_PLATFORM: &str = N_!("Reprise Engine and Linux Platform");

// Native offline Help dialog and its keyboard shortcut descriptions.
pub const HELP: &str = N_!("Help");
pub const NAVIGATION: &str = N_!("Navigation");
pub const PLAY_OR_PAUSE: &str = N_!("Play or Pause");
pub const SEARCH_LIBRARY: &str = N_!("Search Library");
pub const TOGGLE_COMPACT_VIEW: &str = N_!("Toggle Compact View");
pub const CLEAR_SEARCH_OR_RETURN_TO_TRACK_LIST: &str = N_!("Clear Search or Return to Track List");
pub const PLAY_SELECTED_TRACK: &str = N_!("Play Selected Track");
pub const OPEN_CONTEXT_MENU: &str = N_!("Open Context Menu");
pub const OPEN_HELP: &str = N_!("Open Help");

// Primary menu items.
pub const RESCAN_LIBRARY: &str = N_!("Rescan Library");
pub const CANCEL_SCAN: &str = N_!("Cancel Scan");
pub const SYNC_DEVICE: &str = N_!("Sync Device…");
pub const KEYBOARD_SHORTCUTS: &str = N_!("Keyboard Shortcuts");
pub const OPEN_KEYBOARD_SHORTCUTS: &str = N_!("Open Keyboard Shortcuts");

// Compact menu items.
pub const ALWAYS_ON_TOP: &str = N_!("Always on Top");
pub const QUIT: &str = N_!("Quit");

// Color scheme (dark/light/system preference).
pub const COLOR_SCHEME: &str = N_!("Color Scheme");
pub const COLOR_SCHEME_SUBTITLE: &str = N_!("Choose light, dark, or follow system preference");
pub const SCHEME_LIGHT: &str = N_!("Light");
pub const SCHEME_DARK: &str = N_!("Dark");
pub const SCHEME_SYSTEM: &str = N_!("System");
