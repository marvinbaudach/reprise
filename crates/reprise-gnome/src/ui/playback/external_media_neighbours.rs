//! Frozen episode-neighbour navigation for external podcast playback.

use std::rc::Rc;

use reprise_core::media_integration::MprisPlaybackStatus;
use reprise_core::playback::PlaybackState;
use reprise_core::podcasts::EpisodeRow;

use crate::ui::player_controller::PlayerController;

use super::external_media::media_from_episode;
use super::external_media_state::{
    AdvanceFailure, AutomaticAdvance, ExternalSession, NeighbourContext, NeighbourDirection,
    NeighbourTransport, PodcastFailureAction, PodcastPhase,
};
use super::preview::PlaybackMode;

impl PlayerController {
    pub(in crate::ui) fn play_podcast_episode(
        self: &Rc<Self>,
        episode: &EpisodeRow,
        episode_ids: &[i64],
    ) {
        let neighbours = NeighbourContext::for_episode(episode_ids, episode.id);
        if let Err(error) =
            self.play_external_with_context(media_from_episode(episode), neighbours, None)
        {
            self.show_toast(&error.to_string());
        }
    }

    pub(in crate::ui) fn transport_previous(self: &Rc<Self>) {
        if !self.play_external_neighbour(NeighbourDirection::Previous) {
            self.previous();
        }
    }

    pub(in crate::ui) fn transport_next(self: &Rc<Self>) {
        if !self.play_external_neighbour(NeighbourDirection::Next) {
            self.next();
        }
    }

    fn play_external_neighbour(self: &Rc<Self>, direction: NeighbourDirection) -> bool {
        let target = {
            let external = self.external.borrow();
            external.transport_target(direction)
        };
        match target {
            NeighbourTransport::Queue => false,
            NeighbourTransport::Episode(neighbours) => {
                self.play_episode_from_neighbour(neighbours, AutomaticAdvance::new(direction));
                true
            }
            NeighbourTransport::Unavailable => true,
        }
    }

    pub(super) fn play_episode_from_neighbour(
        self: &Rc<Self>,
        neighbours: NeighbourContext,
        automatic_advance: AutomaticAdvance,
    ) {
        let episode_id = neighbours.current_id();
        let episode = reprise_core::podcasts::store::episode(&self.conn, episode_id);
        match episode {
            Ok(Some(episode)) => {
                let _ = self.play_podcast_row_with_context(episode, neighbours, automatic_advance);
            }
            Ok(None) => self.continue_after_advance_failure(
                &neighbours,
                automatic_advance,
                "The neighbouring episode is no longer available",
            ),
            Err(error) => self.continue_after_advance_failure(
                &neighbours,
                automatic_advance,
                &error.to_string(),
            ),
        }
    }

    fn continue_after_advance_failure(
        self: &Rc<Self>,
        neighbours: &NeighbourContext,
        automatic_advance: AutomaticAdvance,
        message: &str,
    ) {
        match automatic_advance.after_failure(neighbours) {
            AdvanceFailure::Retry { neighbours, chain } => {
                self.play_episode_from_neighbour(neighbours, chain);
            }
            AdvanceFailure::Stop => {
                self.stop_external();
                self.show_toast(message);
            }
        }
    }

    pub(super) fn fail_podcast(self: &Rc<Self>, generation: u64, message: &str) {
        if !self.external_generation_matches(generation, PlaybackMode::Podcast) {
            return;
        }
        let failure_action = {
            let mut external = self.external.borrow_mut();
            let Some(ExternalSession::Podcast(session)) = external.session.as_mut() else {
                return;
            };
            let action = session.failure_action();
            if action == PodcastFailureAction::Direct {
                session.phase = PodcastPhase::Failed;
                session.error = Some(message.to_owned());
            }
            action
        };
        if let PodcastFailureAction::Automatic(automatic_failure) = failure_action {
            match automatic_failure {
                AdvanceFailure::Retry { neighbours, chain } => {
                    self.play_episode_from_neighbour(neighbours, chain);
                    return;
                }
                AdvanceFailure::Stop => {
                    if let Some(ExternalSession::Podcast(session)) =
                        self.external.borrow_mut().session.as_mut()
                    {
                        session.phase = PodcastPhase::Failed;
                        session.error = Some(message.to_owned());
                        session.automatic_advance = None;
                    }
                }
            }
        }
        self.show_toast(message);
        self.sync_state(PlaybackState::Stopped);
        self.update_external_mpris(MprisPlaybackStatus::Stopped);
        self.notify_external_changed();
    }
}
