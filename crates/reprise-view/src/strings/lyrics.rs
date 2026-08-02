//! Argument-free labels for the Lyrics surface.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lyrics_labels_preserve_the_existing_catalog_msgids() {
        assert_eq!(
            [
                LYRICS,
                PLAY_TO_SEE_LYRICS,
                LOADING_LYRICS,
                INSTRUMENTAL,
                NO_LYRICS_FOUND,
                LYRICS_UNAVAILABLE,
                RETRY,
                SYNCED_TAGS,
                SYNCED_SIDECAR,
                LYRICS_SIDECAR,
                SYNCED_LRCLIB,
                LYRICS_LRCLIB,
                SYNCED_NETEASE,
                LYRICS_NETEASE,
                LYRICS_TAGS,
                ONLINE_LYRICS_DISABLED,
                ENABLE_LYRICS_DESCRIPTION,
                ENABLE_IN_SETTINGS,
            ],
            [
                "Lyrics",
                "Play a track to see its lyrics",
                "Loading lyrics…",
                "Instrumental",
                "No lyrics found",
                "Could not load lyrics",
                "Retry",
                "synced · tags",
                "synced · .lrc",
                "lyrics · .lrc",
                "synced · LRCLIB",
                "lyrics · LRCLIB",
                "synced · NetEase",
                "lyrics · NetEase",
                "lyrics · tags",
                "Online lyrics are disabled",
                "Enable them to load missing lyrics automatically",
                "Enable in Settings",
            ]
        );
    }
}
