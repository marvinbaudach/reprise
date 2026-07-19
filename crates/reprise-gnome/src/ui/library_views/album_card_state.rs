//! Pure album-card playback presentation decisions.

use std::collections::HashMap;

use reprise_core::playback::PlaybackState;
use reprise_core::queries::AlbumSummary;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui) struct PendingAlbumReveal {
    pub album: String,
    pub artist: String,
    pub generation: u64,
}

impl PendingAlbumReveal {
    pub fn matches(&self, album: &AlbumSummary) -> bool {
        album.album.eq_ignore_ascii_case(&self.album)
            && album.album_artist.eq_ignore_ascii_case(&self.artist)
    }
}

pub(in crate::ui) fn album_index(
    albums: &[AlbumSummary],
    title: &str,
    artist: &str,
) -> Option<u32> {
    albums
        .iter()
        .position(|album| {
            album.album.eq_ignore_ascii_case(title)
                && album.album_artist.eq_ignore_ascii_case(artist)
        })
        .and_then(|index| u32::try_from(index).ok())
}

/// Tracks which reveal generation currently owns a recycled card widget.
/// A timeout may clear the highlight only while its own generation is still
/// bound to that widget; a later bind must survive an older timeout.
#[derive(Default)]
pub(in crate::ui) struct RevealBindingRegistry {
    entries: HashMap<usize, u64>,
}

impl RevealBindingRegistry {
    pub fn bind(&mut self, card_key: usize, generation: u64) {
        self.entries.insert(card_key, generation);
    }

    pub fn take_if_current(&mut self, card_key: usize, generation: u64) -> bool {
        if self.entries.get(&card_key) != Some(&generation) {
            return false;
        }
        self.entries.remove(&card_key);
        true
    }

    pub fn unbind_current(&mut self, card_key: usize) {
        self.entries.remove(&card_key);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum AlbumCardPlayback {
    Normal,
    LoadedStopped,
    Playing,
    Paused,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::ui) struct AlbumCardPresentation {
    pub show_playing_layer: bool,
    pub playback_paused: bool,
}

pub(in crate::ui) fn presentation(playback: AlbumCardPlayback) -> AlbumCardPresentation {
    match playback {
        AlbumCardPlayback::Normal => AlbumCardPresentation::default(),
        AlbumCardPlayback::LoadedStopped => AlbumCardPresentation {
            show_playing_layer: true,
            playback_paused: false,
        },
        AlbumCardPlayback::Playing => AlbumCardPresentation {
            show_playing_layer: true,
            playback_paused: false,
        },
        AlbumCardPlayback::Paused => AlbumCardPresentation {
            show_playing_layer: true,
            playback_paused: true,
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum PrimaryAlbumAction {
    RebuildQueue,
    Pause,
    Resume,
}

pub(in crate::ui) fn primary_album_action(
    is_current_album: bool,
    playback: PlaybackState,
) -> PrimaryAlbumAction {
    if !is_current_album {
        return PrimaryAlbumAction::RebuildQueue;
    }
    match playback {
        PlaybackState::Playing => PrimaryAlbumAction::Pause,
        PlaybackState::Paused => PrimaryAlbumAction::Resume,
        PlaybackState::Stopped => PrimaryAlbumAction::RebuildQueue,
    }
}

#[derive(Default)]
pub(in crate::ui) struct AlbumCardIdentityRegistry {
    entries: HashMap<usize, (u64, AlbumSummary)>,
}

impl AlbumCardIdentityRegistry {
    pub fn bind(&mut self, card_key: usize, generation: u64, album: AlbumSummary) {
        self.entries.insert(card_key, (generation, album));
    }

    pub fn unbind(&mut self, card_key: usize, generation: u64) {
        if self
            .entries
            .get(&card_key)
            .is_some_and(|(bound_generation, _)| *bound_generation == generation)
        {
            self.entries.remove(&card_key);
        }
    }

    pub fn unbind_current(&mut self, card_key: usize) {
        let generation = self
            .entries
            .get(&card_key)
            .map(|(generation, _)| *generation);
        if let Some(generation) = generation {
            self.unbind(card_key, generation);
        }
    }

    pub fn resolve(&self, card_key: usize) -> Option<AlbumSummary> {
        self.entries.get(&card_key).map(|(_, album)| album.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn album(title: &str) -> AlbumSummary {
        AlbumSummary {
            album: title.into(),
            album_artist: "Artist".into(),
            representative_path: String::new(),
            track_count: 1,
            year: None,
            total_duration_ms: 0,
            max_added_at: 0,
            total_play_count: 0,
        }
    }

    #[test]
    fn playing_presentation_keeps_the_badge_visible_while_paused() {
        assert_eq!(
            presentation(AlbumCardPlayback::Normal),
            AlbumCardPresentation {
                show_playing_layer: false,
                playback_paused: false,
            }
        );
        assert_eq!(
            presentation(AlbumCardPlayback::Playing),
            AlbumCardPresentation {
                show_playing_layer: true,
                playback_paused: false,
            }
        );
        assert_eq!(
            presentation(AlbumCardPlayback::Paused),
            AlbumCardPresentation {
                show_playing_layer: true,
                playback_paused: true,
            }
        );
        assert_eq!(
            presentation(AlbumCardPlayback::LoadedStopped),
            AlbumCardPresentation {
                show_playing_layer: true,
                playback_paused: false,
            }
        );
    }

    #[test]
    fn card_identity_registry_ignores_a_stale_unbind_after_recycling() {
        let mut registry = AlbumCardIdentityRegistry::default();
        registry.bind(7, 1, album("Old"));
        registry.bind(7, 2, album("New"));

        registry.unbind(7, 1);
        assert_eq!(registry.resolve(7).unwrap().album, "New");

        registry.unbind(7, 2);
        assert!(registry.resolve(7).is_none());
    }

    #[test]
    fn primary_button_rebuilds_other_or_stopped_and_toggles_the_loaded_album() {
        assert_eq!(
            primary_album_action(false, PlaybackState::Playing),
            PrimaryAlbumAction::RebuildQueue
        );
        assert_eq!(
            primary_album_action(true, PlaybackState::Stopped),
            PrimaryAlbumAction::RebuildQueue
        );
        assert_eq!(
            primary_album_action(true, PlaybackState::Playing),
            PrimaryAlbumAction::Pause
        );
        assert_eq!(
            primary_album_action(true, PlaybackState::Paused),
            PrimaryAlbumAction::Resume
        );
    }

    #[test]
    fn reveal_target_resolves_the_canonical_case_insensitive_model_index() {
        let albums = [album("First"), album("Playing"), album("Last")];

        assert_eq!(album_index(&albums, "playing", "artist"), Some(1));
        assert_eq!(album_index(&albums, "missing", "artist"), None);
        assert!(PendingAlbumReveal {
            album: "PLAYING".into(),
            artist: "ARTIST".into(),
            generation: 7,
        }
        .matches(&albums[1]));
    }

    #[test]
    fn reveal_timeout_cannot_clear_a_newer_recycled_card_generation() {
        let mut registry = RevealBindingRegistry::default();
        registry.bind(7, 1);
        registry.bind(7, 2);

        assert!(!registry.take_if_current(7, 1));
        assert!(registry.take_if_current(7, 2));
        assert!(!registry.take_if_current(7, 2));
    }
}
