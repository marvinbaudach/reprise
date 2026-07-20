macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

pub const REVEAL_PLAYING_ALBUM: &str = N_!("Reveal playing album");

pub const ABOUT_REPRISE: &str = N_!("About Reprise");
pub const REPRISE_ENGINE_AND_LINUX_PLATFORM: &str = N_!("Reprise Engine and Linux Platform");

// Native offline Help dialog and its keyboard shortcut descriptions.
pub const HELP: &str = N_!("Help");
pub const NAVIGATION: &str = N_!("Navigation");
pub const PLAY_OR_PAUSE: &str = N_!("Play or Pause");
pub const STOP_PLAYBACK: &str = N_!("Stop Playback");
pub const INCREASE_VOLUME: &str = N_!("Increase Volume");
pub const DECREASE_VOLUME: &str = N_!("Decrease Volume");
pub const SEARCH_LIBRARY: &str = N_!("Search Library");
pub const TOGGLE_COMPACT_VIEW: &str = N_!("Toggle Compact View");
pub const CLEAR_SEARCH_OR_RETURN_TO_CONTENT: &str = N_!("Clear Search or Return to Content");
pub const PLAY_SELECTED_TRACK: &str = N_!("Play Selected Track");
pub const OPEN_CONTEXT_MENU: &str = N_!("Open Context Menu");
pub const OPEN_HELP: &str = N_!("Open Help");
pub const OPEN_MAIN_MENU: &str = N_!("Open Main Menu");
pub const CLOSE_WINDOW: &str = N_!("Close Window");
pub const QUIT_REPRISE: &str = N_!("Quit Reprise");

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
