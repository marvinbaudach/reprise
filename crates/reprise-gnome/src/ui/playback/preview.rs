//! Shared playback-mode model for queue and external-media sessions.

use reprise_core::media_integration::MprisPlaybackStatus;
use reprise_core::playback::PlaybackState;

use crate::ui::enumerated::enumerated;
use crate::ui::player_controller::PlayerController;

enumerated! {
    /// Whether the controller is playing an ordinary queue track or external media.
    /// `Queue` is the default (ordinary playback).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub(in crate::ui) enum PlaybackMode {
        #[default]
        Queue,
        QueuedEpisode,
        Preview,
        Podcast,
        Radio,
    }

    /// Every mode, generated from the declaration above so a new one cannot
    /// stay out of the `PLAY-12` link contract it is checked against.
    #[allow(dead_code)] // Exhaustive contract exercised by PLAY-12 tests.
    pub(in crate::ui) const Self::ALL;
}

impl PlaybackMode {
    /// Whether a `TrackFinished` event in this mode should advance the queue.
    /// Only a queue track does; a finished external-media session stops instead.
    pub(in crate::ui) fn advances_queue_on_finish(self) -> bool {
        matches!(self, PlaybackMode::Queue | PlaybackMode::QueuedEpisode)
    }

    pub(in crate::ui) fn credits_listening(self) -> bool {
        matches!(self, PlaybackMode::Queue)
    }

    /// Whether the audio-reactive apparatus — the spectrum feed, the Visual
    /// tab and the reactive light — has anything to show in this mode.
    ///
    /// Podcasts are speech: a spectrum of a voice is a flicker, not a visual,
    /// so both podcast modes switch the whole chain off at the source. Radio
    /// plays music and keeps it. This is the ONE place that names which modes
    /// count as a podcast — `audio_reactive_enabled` reads the answer, nobody
    /// re-derives it.
    pub(in crate::ui) fn runs_audio_visuals(self) -> bool {
        !matches!(self, PlaybackMode::Podcast | PlaybackMode::QueuedEpisode)
    }
}

/// The whole audio-reactive question in one expression: the module the user
/// can switch off in Preferences, AND a mode that has something to show.
///
/// Pure and free-standing because the controller it answers for cannot be
/// built outside a window — this is where the rule can actually be tested.
pub(in crate::ui) fn audio_reactive_enabled(module_enabled: bool, mode: PlaybackMode) -> bool {
    module_enabled && mode.runs_audio_visuals()
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
    use super::{audio_reactive_enabled, PlaybackMode};

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
    fn pod_24_external_session_never_scrobbles() {
        assert!(PlaybackMode::Queue.credits_listening());
        assert!(!PlaybackMode::QueuedEpisode.credits_listening());
        assert!(!PlaybackMode::Preview.credits_listening());
        assert!(!PlaybackMode::Podcast.credits_listening());
    }

    #[test]
    fn que_9_queued_episode_advances_without_earning_a_listen() {
        assert!(PlaybackMode::QueuedEpisode.advances_queue_on_finish());
        assert!(!PlaybackMode::QueuedEpisode.credits_listening());
    }

    #[test]
    fn rad_2_external_session_never_scrobbles() {
        assert!(!PlaybackMode::Radio.credits_listening());
    }

    // Every mode is named, so a sixth one cannot slip into the visuals
    // contract unexamined: a podcast — direct or queued — runs no audio
    // visuals, everything else does.
    #[test]
    fn podcasts_run_no_audio_visuals_while_music_and_radio_do() {
        for mode in PlaybackMode::ALL {
            // Spelled out arm by arm with no wildcard: a sixth mode stops the
            // build here rather than inheriting an answer by accident.
            let expected = match mode {
                PlaybackMode::Queue => true,
                PlaybackMode::Preview => true,
                PlaybackMode::Radio => true,
                PlaybackMode::Podcast => false,
                PlaybackMode::QueuedEpisode => false,
            };
            assert_eq!(
                mode.runs_audio_visuals(),
                expected,
                "{mode:?} disagrees with the podcast rule"
            );
        }
    }

    // The two inputs, all four corners: a podcast suppresses the visuals even
    // with the module on, and the module still wins over every other mode.
    #[test]
    fn a_podcast_suppresses_the_visuals_the_module_switch_would_allow() {
        assert!(audio_reactive_enabled(true, PlaybackMode::Queue));
        assert!(audio_reactive_enabled(true, PlaybackMode::Radio));
        assert!(!audio_reactive_enabled(true, PlaybackMode::Podcast));
        assert!(!audio_reactive_enabled(true, PlaybackMode::QueuedEpisode));

        assert!(!audio_reactive_enabled(false, PlaybackMode::Queue));
        assert!(!audio_reactive_enabled(false, PlaybackMode::Radio));
        assert!(!audio_reactive_enabled(false, PlaybackMode::Podcast));
    }
}
