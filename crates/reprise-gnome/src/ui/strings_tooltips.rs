macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

use super::{formatted, text};

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
