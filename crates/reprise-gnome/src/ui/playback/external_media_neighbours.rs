//! Frozen episode-neighbour navigation for external podcast playback.

use std::rc::Rc;

use reprise_core::media_integration::MprisPlaybackStatus;
use reprise_core::playback::PlaybackState;
use reprise_core::podcasts::EpisodeRow;
use reprise_core::up_next::QueueItem;

use crate::ui::current_track_selection::CurrentTrackChange;
use crate::ui::player_controller::PlayerController;
use crate::ui::player_controller::StartPlayback;

use super::external_media::media_from_episode;
use super::external_media_state::{
    should_skip_manual_queue_after_failure, AdvanceFailure, AutomaticAdvance, ExternalSession,
    NeighbourContext, NeighbourDirection, NeighbourTransport, PodcastFailureAction, PodcastOrigin,
    PodcastPhase,
};

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
            NeighbourTransport::Queue | NeighbourTransport::History => false,
            NeighbourTransport::Item { neighbours, origin } => {
                self.play_item_from_neighbour(neighbours, AutomaticAdvance::new(direction), origin);
                true
            }
            NeighbourTransport::Unavailable => true,
        }
    }

    pub(super) fn jump_to_direct_episode_context(self: &Rc<Self>, offset: usize) -> bool {
        let target = {
            let external = self.external.borrow();
            let Some(ExternalSession::Podcast(session)) = external.session.as_ref() else {
                return false;
            };
            if session.origin != PodcastOrigin::Direct {
                return false;
            }
            session
                .neighbours
                .as_ref()
                .and_then(|neighbours| neighbours.upcoming_context(offset))
        };
        let Some(neighbours) = target else {
            tracing::debug!(
                offset,
                "direct episode context jump target vanished; ignoring"
            );
            return true;
        };
        self.play_item_from_neighbour(
            neighbours,
            AutomaticAdvance::new(NeighbourDirection::Next),
            PodcastOrigin::Direct,
        );
        true
    }

    pub(super) fn play_item_from_neighbour(
        self: &Rc<Self>,
        neighbours: NeighbourContext,
        automatic_advance: AutomaticAdvance,
        origin: PodcastOrigin,
    ) {
        let item = neighbours.current_item();
        if origin == PodcastOrigin::ManualQueue {
            self.consume_manual_neighbour(item);
        }
        let QueueItem::Episode(episode_id) = item else {
            self.present_queue_item(
                item,
                StartPlayback::Yes,
                CurrentTrackChange::ExplicitTransport,
            );
            return;
        };
        let episode = reprise_core::podcasts::store::episode(&self.conn, episode_id);
        match episode {
            Ok(Some(episode)) => {
                let _ = self.play_podcast_row_with_context(
                    &episode,
                    neighbours,
                    automatic_advance,
                    origin,
                );
            }
            Ok(None) => self.continue_after_advance_failure(
                &neighbours,
                automatic_advance,
                origin,
                "The neighbouring episode is no longer available",
            ),
            Err(error) => self.continue_after_advance_failure(
                &neighbours,
                automatic_advance,
                origin,
                &error.to_string(),
            ),
        }
    }

    fn consume_manual_neighbour(&self, item: QueueItem) {
        let removed = {
            let mut pending = self.up_next.borrow_mut();
            let position = pending
                .ids()
                .iter()
                .position(|candidate| *candidate == item);
            position.and_then(|position| pending.take_at(position))
        };
        self.current_up_next.set(Some(item));
        if removed.is_some() {
            self.notify_queue_changed();
        }
    }

    fn continue_after_advance_failure(
        self: &Rc<Self>,
        neighbours: &NeighbourContext,
        automatic_advance: AutomaticAdvance,
        origin: PodcastOrigin,
        message: &str,
    ) {
        match automatic_advance.after_failure(neighbours) {
            AdvanceFailure::Retry { neighbours, chain } => {
                self.play_item_from_neighbour(neighbours, chain, origin);
            }
            AdvanceFailure::Stop => {
                self.stop_external();
                self.show_toast(message);
            }
        }
    }

    pub(super) fn fail_podcast(self: &Rc<Self>, generation: u64, message: &str) {
        let (failure_action, origin) = {
            let mut external = self.external.borrow_mut();
            if external.generation != generation {
                return;
            }
            let Some(ExternalSession::Podcast(session)) = external.session.as_mut() else {
                return;
            };
            let action = session.failure_action();
            if action == PodcastFailureAction::Direct {
                session.phase = PodcastPhase::Failed;
                session.error = Some(message.to_owned());
            }
            (action, session.origin)
        };
        let skip_manual_queue = should_skip_manual_queue_after_failure(origin, &failure_action);
        if let PodcastFailureAction::Automatic(automatic_failure) = failure_action {
            match automatic_failure {
                AdvanceFailure::Retry { neighbours, chain } => {
                    self.play_item_from_neighbour(neighbours, chain, origin);
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
        if skip_manual_queue {
            super::playback_faults::note_episode_skip(&self.consecutive_episode_skips);
            self.external.borrow_mut().clear_session();
            self.notify_external_changed();
            self.skip_after_failure();
            return;
        }
        self.show_toast(message);
        self.sync_state(PlaybackState::Stopped);
        self.update_external_mpris(MprisPlaybackStatus::Stopped);
        self.notify_external_changed();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn unavailable_external_transport_is_consumed_as_a_no_op() {
        let implementation = include_str!("external_media_neighbours.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("implementation section");
        let handled = ["NeighbourTransport::Unavailable => ", "true"].concat();

        assert!(implementation.contains(&handled));
    }
}
