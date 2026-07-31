//! Pure playback decisions shared by the podcast episode surfaces.

use crate::ui::playback::external_media::PodcastPhase;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) struct EpisodeMark {
    pub(in crate::ui) id: i64,
    pub(in crate::ui) playing: bool,
}

impl EpisodeMark {
    pub(in crate::ui) fn new(id: i64, playing: bool) -> Self {
        Self { id, playing }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EpisodeActivation {
    StartEpisode,
    TogglePlayback,
}

/// `POD-20`: the one place that decides what activating a row means. The
/// loaded episode toggles; everything else starts. Kept here, off the row
/// widgets, so the two episode surfaces cannot drift into two answers —
/// re-deriving it per widget is what made a restart pass for a toggle.
pub(super) fn activation_for_episode(
    loaded_episode_id: Option<i64>,
    activated_episode_id: i64,
) -> EpisodeActivation {
    if loaded_episode_id == Some(activated_episode_id) {
        EpisodeActivation::TogglePlayback
    } else {
        EpisodeActivation::StartEpisode
    }
}

pub(in crate::ui) fn podcast_phase_is_playing(phase: Option<PodcastPhase>) -> bool {
    matches!(phase, Some(PodcastPhase::Resolving | PodcastPhase::Playing))
}

pub(super) fn episode_mark_requires_render(
    previous: Option<EpisodeMark>,
    next: Option<EpisodeMark>,
) -> bool {
    previous.map(|mark| mark.id) != next.map(|mark| mark.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pod_20_activating_the_loaded_episode_toggles_without_starting_a_new_session() {
        assert_eq!(
            activation_for_episode(Some(41), 41),
            EpisodeActivation::TogglePlayback
        );
        assert_eq!(
            activation_for_episode(Some(41), 42),
            EpisodeActivation::StartEpisode
        );
        assert_eq!(
            activation_for_episode(None, 41),
            EpisodeActivation::StartEpisode
        );
    }

    #[test]
    fn pod_20_podcast_phase_maps_to_the_visible_playing_state() {
        assert!(podcast_phase_is_playing(Some(PodcastPhase::Resolving)));
        assert!(podcast_phase_is_playing(Some(PodcastPhase::Playing)));
        assert!(!podcast_phase_is_playing(Some(PodcastPhase::Paused)));
        assert!(!podcast_phase_is_playing(Some(PodcastPhase::Failed)));
        assert!(!podcast_phase_is_playing(None));
    }

    #[test]
    fn only_an_episode_identity_change_requires_a_full_render() {
        let playing = Some(EpisodeMark::new(41, true));
        let paused = Some(EpisodeMark::new(41, false));
        let other = Some(EpisodeMark::new(42, true));

        assert!(!episode_mark_requires_render(playing, paused));
        assert!(!episode_mark_requires_render(playing, playing));
        assert!(episode_mark_requires_render(playing, other));
        assert!(episode_mark_requires_render(playing, None));
        assert!(!episode_mark_requires_render(None, None));
    }
}
