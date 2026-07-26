//! Player-bar sensitivity derived from playback, queue, and library state.

use reprise_core::playback::PlaybackState;

pub(in crate::ui) fn bar_should_be_sensitive(
    state: PlaybackState,
    queue_has_tracks: bool,
    library_has_tracks: bool,
) -> bool {
    state != PlaybackState::Stopped || queue_has_tracks || library_has_tracks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_9_stopped_bar_is_enabled_when_the_library_can_start() {
        assert!(bar_should_be_sensitive(PlaybackState::Stopped, false, true));
        assert!(!bar_should_be_sensitive(
            PlaybackState::Stopped,
            false,
            false
        ));
    }
}
