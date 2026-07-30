//! Pure playback decisions shared by the podcast episode surfaces.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EpisodeActivation {
    StartEpisode,
    TogglePlayback,
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activating_the_loaded_episode_toggles_without_starting_a_new_session() {
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
}
