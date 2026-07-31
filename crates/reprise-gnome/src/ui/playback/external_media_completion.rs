//! Podcast completion policy, separated from source resolution and playback.

use std::rc::Rc;

use reprise_core::media_integration::MprisPlaybackStatus;
use reprise_core::playback::PlaybackState;

use super::external_media_state::ExternalSession;
use super::preview::PlaybackMode;
use crate::ui::player_controller::PlayerController;

impl PlayerController {
    pub(in crate::ui) fn finish_external(self: &Rc<Self>) {
        match self.playback_mode() {
            PlaybackMode::Podcast => self.finish_podcast(),
            PlaybackMode::QueuedEpisode => self.finish_queued_episode(),
            PlaybackMode::Radio => self.handle_external_error("Radio stream ended".into()),
            PlaybackMode::Preview => self.end_preview(),
            PlaybackMode::Queue => {}
        }
    }

    fn finish_podcast(self: &Rc<Self>) {
        let finished = {
            let external = self.external.borrow();
            let Some(ExternalSession::Podcast(session)) = external.session.as_ref() else {
                return;
            };
            (
                super::external_media::session_id(&session.media),
                session.subscription_id,
                session.published_at,
            )
        };
        let now = chrono::Utc::now().timestamp();
        if let Err(error) = reprise_core::podcasts::store::mark_played(&self.conn, finished.0, now)
        {
            tracing::error!(%error, episode_id = finished.0, "could not mark podcast played");
        }
        let next = reprise_core::podcasts::query::next_unplayed_of_show(
            &self.conn, finished.1, finished.2,
        )
        .ok()
        .flatten();
        let callbacks = {
            let mut external = self.external.borrow_mut();
            external.play_next = next.clone();
            external.clear_session();
            external.play_next_callbacks.clone()
        };
        if let Some(next) = next {
            self.show_play_next_offer(&next);
            for callback in callbacks {
                callback(next.clone());
            }
        }
        self.update_mpris_mirror(MprisPlaybackStatus::Stopped);
        self.sync_state(PlaybackState::Stopped);
        self.sync_clear_track();
        self.notify_external_changed();
    }

    fn finish_queued_episode(self: &Rc<Self>) {
        let episode_id = {
            let external = self.external.borrow();
            let Some(ExternalSession::Podcast(session)) = external.session.as_ref() else {
                return;
            };
            super::external_media::session_id(&session.media)
        };
        let now = chrono::Utc::now().timestamp();
        if let Err(error) = reprise_core::podcasts::store::mark_played(&self.conn, episode_id, now)
        {
            tracing::error!(%error, episode_id, "could not mark queued podcast played");
        }
        self.external.borrow_mut().clear_session();
        self.notify_external_changed();
        self.advance_playback(super::up_next_transport::AdvanceReason::Automatic);
    }
}
