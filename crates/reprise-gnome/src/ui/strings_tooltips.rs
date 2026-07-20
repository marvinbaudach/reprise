macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

use super::{formatted, text};

pub const SHORTCUT_SEARCH: &str = N_!("Ctrl+F");
pub const SHORTCUT_MAIN_MENU: &str = N_!("F10");
pub const SHORTCUT_COMPACT_MODE: &str = N_!("Ctrl+M");

pub fn shortcut_tooltip(message: &str, shortcut: &str) -> String {
    append_shortcut(&text(message), &text(shortcut))
}

fn append_shortcut(label: &str, shortcut: &str) -> String {
    format!("{label} ({shortcut})")
}

/// Tooltip of the album-card hover-overlay play button (TIP-1a).
pub const PLAY_ALBUM: &str = N_!("Play album (Ctrl+Enter)");
pub const PAUSE_ALBUM: &str = N_!("Pause album");
pub const RESUME_ALBUM: &str = N_!("Resume album");

/// Transport tooltips (TIP-1b): verb + object, shortcut in parentheses.
/// PLAY/PAUSE/PREVIOUS/NEXT stay as menu labels (compact player menu).
pub const TOOLTIP_PLAY: &str = N_!("Play (Space)");
pub const TOOLTIP_PAUSE: &str = N_!("Pause (Space)");
pub const TOOLTIP_PREVIOUS: &str = N_!("Play previous track");
pub const TOOLTIP_NEXT: &str = N_!("Play next track");

/// Mini-player hover-overlay buttons (icon-only, TIP-1a/1b).
pub const TOOLTIP_RESTORE_FULL_WINDOW: &str = N_!("Restore full window (Ctrl+M)");
pub const TOOLTIP_CLOSE_MINI_PLAYER: &str = N_!("Close mini-player");

// Scan sidebar-toggle and card tooltips (dynamic values allowed per TIP-5).

pub fn scan_card_tooltip(remaining: u64) -> String {
    let remaining = remaining.to_string();
    formatted(
        N_!("Covers & lyrics: {remaining} queued"),
        &[("remaining", &remaining)],
    )
}

pub fn scan_tooltip_discovering() -> String {
    text(N_!("Scanning\u{2026}"))
}

pub fn scan_tooltip_progress(pct: u32) -> String {
    let pct = pct.to_string();
    formatted(N_!("Scanning \u{00B7} {pct}%"), &[("pct", &pct)])
}

#[cfg(test)]
mod tests {
    use super::{
        append_shortcut, PLAY_ALBUM, TOOLTIP_PAUSE, TOOLTIP_PLAY, TOOLTIP_RESTORE_FULL_WINDOW,
    };

    #[test]
    fn tip_6_controls_show_only_their_existing_action_shortcuts() {
        assert_eq!(
            append_shortcut("Search all fields", "Ctrl+F"),
            "Search all fields (Ctrl+F)"
        );
        for (tooltip, shortcut) in [
            (TOOLTIP_PLAY, "Space"),
            (TOOLTIP_PAUSE, "Space"),
            (PLAY_ALBUM, "Ctrl+Enter"),
            (TOOLTIP_RESTORE_FULL_WINDOW, "Ctrl+M"),
        ] {
            assert!(
                tooltip.ends_with(&format!("({shortcut})")),
                "`{tooltip}` must expose `{shortcut}`"
            );
        }

        let contracts = [
            (
                "Library search",
                include_str!("window/library_chrome.rs"),
                "strings::shortcut_tooltip(strings::SEARCH_PLACEHOLDER,strings::SHORTCUT_SEARCH,)",
            ),
            (
                "primary menu",
                include_str!("primary_menu.rs"),
                "strings::shortcut_tooltip(strings::MAIN_MENU,strings::SHORTCUT_MAIN_MENU,)",
            ),
            (
                "mini-player close",
                include_str!("compact/compact_player_layouts.rs"),
                "close_button.set_tooltip_text(Some(&strings::shortcut_tooltip(strings::TOOLTIP_CLOSE_MINI_PLAYER,strings::SHORTCUT_COMPACT_MODE,)))",
            ),
        ];

        for (control, source, contract) in contracts {
            let compact_source = source.split_whitespace().collect::<String>();
            assert_eq!(
                compact_source.matches(contract).count(),
                1,
                "{control} must expose exactly one matching shortcut tooltip"
            );
        }

        let player_bar = include_str!("player_bar/player_bar_layout.rs");
        assert!(
            !player_bar.contains("SHORTCUT_VOLUME"),
            "full-player volume controls must not claim compact-only shortcuts"
        );
    }
}
