//! Argument-free labels for the Lyrics surface.
//!
//! There is deliberately no unit test pinning these literals. A test that
//! asserts `LYRICS == "Lyrics"` next to `LYRICS = N_!("Lyrics")` compares a
//! constant with a copy of itself two lines below and fails only when someone
//! edits one of the two — it cannot catch what actually matters here, which is
//! a msgid drifting away from the translated catalogs. That is what
//! `scripts/tests/gettext-catalogs.sh` checks, against the real `po/*.po`
//! files, for all seven locales.

pub use super::scan::RETRY;

pub const LYRICS: &str = N_!("Lyrics");
pub const PLAY_TO_SEE_LYRICS: &str = N_!("Play a track to see its lyrics");
pub const LOADING_LYRICS: &str = N_!("Loading lyrics…");
pub const INSTRUMENTAL: &str = N_!("Instrumental");
pub const NO_LYRICS_FOUND: &str = N_!("No lyrics found");
pub const LYRICS_UNAVAILABLE: &str = N_!("Could not load lyrics");
pub const SYNCED_TAGS: &str = N_!("synced · tags");
pub const SYNCED_SIDECAR: &str = N_!("synced · .lrc");
pub const LYRICS_SIDECAR: &str = N_!("lyrics · .lrc");
pub const SYNCED_LRCLIB: &str = N_!("synced · LRCLIB");
pub const LYRICS_LRCLIB: &str = N_!("lyrics · LRCLIB");
pub const SYNCED_NETEASE: &str = N_!("synced · NetEase");
pub const LYRICS_NETEASE: &str = N_!("lyrics · NetEase");
pub const LYRICS_TAGS: &str = N_!("lyrics · tags");
pub const ONLINE_LYRICS_DISABLED: &str = N_!("Online lyrics are disabled");
pub const ENABLE_LYRICS_DESCRIPTION: &str = N_!("Enable them to load missing lyrics automatically");
pub const ENABLE_IN_SETTINGS: &str = N_!("Enable in Settings");
