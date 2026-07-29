//! Shared playback-mode model for queue and external-media sessions.

use reprise_core::media_integration::MprisPlaybackStatus;
use reprise_core::playback::PlaybackState;

use crate::ui::player_controller::PlayerController;

/// Whether the controller is playing an ordinary queue track or external media.
/// `Queue` is the default (ordinary playback).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(in crate::ui) enum PlaybackMode {
    #[default]
    Queue,
    Preview,
    Podcast,
    Radio,
}

impl PlaybackMode {
    /// Whether a `TrackFinished` event in this mode should advance the queue.
    /// Only a queue track does; a finished external-media session stops instead.
    pub(in crate::ui) fn advances_queue_on_finish(self) -> bool {
        matches!(self, PlaybackMode::Queue)
    }

    pub(in crate::ui) fn credits_listening(self) -> bool {
        matches!(self, PlaybackMode::Queue)
    }
}

impl PlayerController {
    /// Leaves preview mode and stops the pipeline without clearing the queue.
    pub(in crate::ui) fn end_preview(&self) {
        self.external.borrow_mut().clear_preview();
        self.evaluate_play_tracking();
        self.update_mpris_mirror(MprisPlaybackStatus::Stopped);
        match self.player.stop() {
            Ok(()) => {}
            Err(error) => {
                tracing::error!(%error, "failed to stop after external preview");
                self.sync_state(PlaybackState::Stopped);
                self.sync_clear_track();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PlaybackMode;

    // A finished preview never advances the queue, while an ordinary queue
    // track does.
    #[test]
    fn preview_mode_finish_stops_while_queue_advances() {
        assert!(
            PlaybackMode::Queue.advances_queue_on_finish(),
            "an ordinary queue track advances the queue when it finishes"
        );
        assert!(
            !PlaybackMode::Preview.advances_queue_on_finish(),
            "a finished preview stops instead of advancing the queue"
        );
        assert!(!PlaybackMode::Podcast.advances_queue_on_finish());
        assert!(!PlaybackMode::Radio.advances_queue_on_finish());
    }

    #[test]
    fn playback_mode_defaults_to_queue() {
        assert_eq!(PlaybackMode::default(), PlaybackMode::Queue);
    }

    #[test]
    fn pod_4_external_session_never_scrobbles() {
        assert!(PlaybackMode::Queue.credits_listening());
        assert!(!PlaybackMode::Preview.credits_listening());
        assert!(!PlaybackMode::Podcast.credits_listening());
    }

    #[test]
    fn rad_2_external_session_never_scrobbles() {
        assert!(!PlaybackMode::Radio.credits_listening());
    }
}
