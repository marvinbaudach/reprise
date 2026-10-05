macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

use super::plural;

pub const REVEAL_PLAYING_ALBUM: &str = N_!("Reveal playing album");

pub const ABOUT_REPRISE: &str = N_!("About Reprise");
pub const TRANSLATOR_CREDITS: &str = N_!("translator-credits");

// Native offline Help dialog and its keyboard shortcut descriptions.
pub const HELP: &str = N_!("Help");
pub const NAVIGATION: &str = N_!("Navigation");
pub const PLAY_OR_PAUSE: &str = N_!("Play or Pause");
pub const INCREASE_VOLUME: &str = N_!("Increase Volume");
pub const DECREASE_VOLUME: &str = N_!("Decrease Volume");
pub const QUICK_OPEN: &str = N_!("Quick Open");
pub const QUICK_OPEN_NO_RESULTS: &str = N_!("No results");
pub const QUICK_OPEN_SEARCH_FAILED: &str = N_!("Quick Open search failed");
pub const QUICK_OPEN_TRACK: &str = N_!("Track");
pub const QUICK_OPEN_TRACKS: &str = N_!("Tracks");
pub const QUICK_OPEN_ALBUM: &str = N_!("Album");
pub const QUICK_OPEN_ALBUMS: &str = N_!("Albums");
pub const QUICK_OPEN_ARTIST: &str = N_!("Artist");
pub const QUICK_OPEN_ARTISTS: &str = N_!("Artists");
pub const QUICK_OPEN_PLAYLIST: &str = N_!("Playlist");
pub const QUICK_OPEN_PLAYLISTS: &str = N_!("Playlists");
pub const QUICK_OPEN_PODCAST_SHOW: &str = N_!("Podcast show");
pub const QUICK_OPEN_PODCAST_SHOWS: &str = N_!("Podcast shows");
pub const QUICK_OPEN_RADIO_STATION: &str = N_!("Radio station");
pub const QUICK_OPEN_RADIO_STATIONS: &str = N_!("Radio stations");
pub const QUICK_OPEN_SMART_PLAYLIST: &str = N_!("Smart playlist");
pub const SLEEP_TIMER: &str = N_!("Sleep Timer");
pub const END_OF_TRACK: &str = N_!("End of Track");
pub const CANCEL_SLEEP_TIMER: &str = N_!("Cancel Sleep Timer");
pub const PAUSES_AFTER_THIS_TRACK: &str = N_!("Pauses after this track");
pub const PAUSED_BY_SLEEP_TIMER: &str = N_!("Paused by sleep timer");
pub const SEARCH_LIBRARY: &str = N_!("Search Library");
pub const ESC_TO_CLOSE: &str = N_!("Esc to close");
pub const TOGGLE_COMPACT_VIEW: &str = N_!("Toggle Compact View");
pub const CLEAR_SEARCH_OR_RETURN_TO_CONTENT: &str = N_!("Clear Search or Return to Content");
pub const PLAY_SELECTED_TRACK: &str = N_!("Play Selected Track");
pub const OPEN_CONTEXT_MENU: &str = N_!("Open Context Menu");
pub const OPEN_HELP: &str = N_!("Open Help");
pub const OPEN_MAIN_MENU: &str = N_!("Open Main Menu");
pub const CLOSE_WINDOW: &str = N_!("Close Window");
pub const QUIT_REPRISE: &str = N_!("Quit Reprise");

// Primary menu items.
pub const CANCEL_SCAN: &str = N_!("Cancel Scan");
pub const KEYBOARD_SHORTCUTS: &str = N_!("Keyboard Shortcuts");
pub const OPEN_KEYBOARD_SHORTCUTS: &str = N_!("Open Keyboard Shortcuts");

// Compact menu items.
pub const ALWAYS_ON_TOP: &str = N_!("Always on Top");
pub const QUIT: &str = N_!("Quit");

pub fn sleep_timer_minutes(minutes: usize) -> String {
    let minutes_text = minutes.to_string();
    plural(
        "{minutes} minute",
        "{minutes} minutes",
        minutes,
        &[("minutes", &minutes_text)],
    )
}

pub fn sleep_timer_pauses_in(minutes: usize) -> String {
    let minutes_text = minutes.to_string();
    plural(
        "Pauses in {minutes} minute",
        "Pauses in {minutes} minutes",
        minutes,
        &[("minutes", &minutes_text)],
    )
}

pub fn sleep_timer_paused() -> String {
    super::text(PAUSED_BY_SLEEP_TIMER)
}

pub fn quick_open_track_count(count: usize) -> String {
    let count_text = count.to_string();
    plural(
        "{count} track",
        "{count} tracks",
        count,
        &[("count", &count_text)],
    )
}

pub fn quick_open_show_all(count: usize, section: &str) -> String {
    super::formatted(
        N_!("Show all {count} in {section}"),
        &[("count", &count.to_string()), ("section", section)],
    )
}

pub fn quick_open_accessible(kind: &str, title: &str, subtitle: &str) -> String {
    if subtitle.is_empty() {
        return format!("{kind}: {title}");
    }
    super::formatted(
        N_!("{kind}: {title}, {subtitle}"),
        &[("kind", kind), ("title", title), ("subtitle", subtitle)],
    )
}

pub fn sidebar_turn_off(name: &str) -> String {
    super::formatted(N_!("Turn Off {name}"), &[("name", name)])
}

pub fn sidebar_module_settings(name: &str) -> String {
    super::formatted(N_!("{name} settings…"), &[("name", name)])
}

pub fn sidebar_turn_off_failed(name: &str) -> String {
    super::formatted(N_!("Could not turn off {name}"), &[("name", name)])
}

pub fn sidebar_turned_off(name: &str) -> String {
    super::formatted(N_!("{name} turned off"), &[("name", name)])
}

pub fn sidebar_turned_off_showing_music(name: &str) -> String {
    super::formatted(N_!("{name} turned off · showing Music"), &[("name", name)])
}

// Appearance accent and color scheme preferences.
pub const ACCENT_COLOR: &str = N_!("Accent Color");
pub const ACCENT_COLOR_SUBTITLE: &str = N_!("Choose the Reprise accent or follow the system");
pub const ACCENT_SOURCE_APP: &str = N_!("App Accent");
pub const COLOR_SCHEME: &str = N_!("Color Scheme");
pub const COLOR_SCHEME_SUBTITLE: &str = N_!("Choose light, dark, or follow system preference");
pub const SCHEME_LIGHT: &str = N_!("Light");
pub const SCHEME_DARK: &str = N_!("Dark");
pub const SCHEME_SYSTEM: &str = N_!("System");

#[cfg(test)]
mod tests {
    #[test]
    fn search_17_accessible_label_has_no_empty_subtitle_punctuation() {
        let label = super::quick_open_accessible("Track", "Silence", "");
        assert!(!label.ends_with(", "));
        assert!(label.contains("Silence"));
    }
}
