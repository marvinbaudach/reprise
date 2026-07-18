//! Pure album-card playback presentation decisions.

use std::collections::HashMap;

use reprise_core::playback::PlaybackState;
use reprise_core::queries::AlbumSummary;

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
}
