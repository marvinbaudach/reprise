//! Player-bar sensitivity derived from playback and queue state.

use reprise_core::playback::PlaybackState;

pub(in crate::ui) fn bar_should_be_sensitive(state: PlaybackState, queue_has_tracks: bool) -> bool {
    state != PlaybackState::Stopped || queue_has_tracks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopped_bar_is_enabled_when_a_queue_can_be_started() {
        assert!(bar_should_be_sensitive(PlaybackState::Stopped, true));
        assert!(!bar_should_be_sensitive(PlaybackState::Stopped, false));
    }
}
