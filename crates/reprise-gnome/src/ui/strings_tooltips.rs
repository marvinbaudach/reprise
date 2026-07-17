macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

/// Tooltip of the album-card hover-overlay play button (TIP-1a).
pub const PLAY_ALBUM: &str = N_!("Play album");

/// Transport tooltips (TIP-1b): verb + object, shortcut in parentheses.
/// PLAY/PAUSE/PREVIOUS/NEXT stay as menu labels (compact player menu).
pub const TOOLTIP_PLAY: &str = N_!("Play (Space)");
pub const TOOLTIP_PAUSE: &str = N_!("Pause (Space)");
pub const TOOLTIP_PREVIOUS: &str = N_!("Play previous track");
pub const TOOLTIP_NEXT: &str = N_!("Play next track");
pub const TOOLTIP_QUEUE: &str = N_!("Show queue");
