//! Podcast completion policy, separated from source resolution and playback.

use std::rc::Rc;

use reprise_core::media_integration::MprisPlaybackStatus;
use reprise_core::playback::PlaybackState;

use super::external_media_state::{
    AutomaticAdvance, ExternalPlaybackState, ExternalSession, NeighbourContext, NeighbourDirection,
    PodcastOrigin, PodcastSession,
};
use super::preview::PlaybackMode;
use crate::ui::player_controller::PlayerController;

fn automatic_completion_target(session: &PodcastSession) -> Option<NeighbourContext> {
    if session.kind != reprise_core::podcasts::PodcastKind::Youtube
        || session.origin != PodcastOrigin::Direct
    {
        return None;
    }
    session.neighbours.as_ref()?.next()
}

fn take_automatic_completion_target(
    external: &mut ExternalPlaybackState,
) -> Option<NeighbourContext> {
    let target = match external.session.as_ref() {
        Some(ExternalSession::Podcast(session)) => automatic_completion_target(session),
        Some(ExternalSession::Radio(_)) | None => None,
    };
    if target.is_some() {
        external.clear_session();
    }
    target
}

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
        let (episode_id, subscription_id, published_at) = {
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
        if let Err(error) = reprise_core::podcasts::store::mark_played(&self.conn, episode_id, now)
        {
            tracing::error!(%error, episode_id, "could not mark podcast played");
        } else {
            // Same reason as in `finish_queued_episode`: a database-backed
            // sidebar count moved, and no other path here recomputes it.
            self.notify_episode_played();
        }
        let automatic_target = {
            let mut external = self.external.borrow_mut();
            take_automatic_completion_target(&mut external)
        };
        if let Some(target) = automatic_target {
            self.play_item_from_neighbour(
                target,
                AutomaticAdvance::new(NeighbourDirection::Next),
                PodcastOrigin::Direct,
            );
            return;
        }
        let next = reprise_core::podcasts::query::next_unplayed_of_show(
            &self.conn,
            subscription_id,
            published_at,
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
        } else {
            // The unplayed counts behind the Podcasts and YouTube rows just
            // dropped. The queue-changed path below only patches the Queue
            // badge, so without this the sidebar would keep showing the old
            // number until some unrelated rebuild happened to fire.
            self.notify_episode_played();
        }
        self.external.borrow_mut().clear_session();
        self.notify_external_changed();
        self.advance_playback(super::up_next_transport::AdvanceReason::Automatic);
    }
}

#[cfg(test)]
mod tests {
    use reprise_core::podcasts::PodcastKind;
    use reprise_core::up_next::QueueItem;

    use super::super::external_media_state::{
        EpisodeSource, ExternalMedia, ExternalPlaybackState, ExternalSession, NeighbourContext,
        PodcastOrigin, PodcastPhase, PodcastSession, ResumePolicy,
    };

    fn direct_session(kind: PodcastKind) -> PodcastSession {
        PodcastSession {
            media: ExternalMedia::Podcast {
                episode_id: 7,
                title: "Episode".into(),
                show: "Channel".into(),
                source: EpisodeSource::Url("https://example.test/watch?v=7".into()),
                resume_ms: 0,
                duration_ms: Some(60_000),
            },
            neighbours: NeighbourContext::for_episode(&[7, 8, 9], 7),
            automatic_advance: None,
            subscription_id: 42,
            kind,
            media_category: None,
            published_at: None,
            art_url: None,
            phase: PodcastPhase::Playing,
            restored: false,
            origin: PodcastOrigin::Direct,
            resume: ResumePolicy::new(0),
            position_ms: 60_000,
            last_persisted_ms: 60_000,
            duration_known: true,
            error: None,
        }
    }

    #[test]
    fn pod_24_direct_youtube_completion_uses_the_frozen_next_episode() {
        let mut youtube = ExternalPlaybackState {
            session: Some(ExternalSession::Podcast(direct_session(
                PodcastKind::Youtube,
            ))),
            ..ExternalPlaybackState::default()
        };
        let target = super::take_automatic_completion_target(&mut youtube)
            .expect("a finished YouTube episode should continue through its visible context");

        assert_eq!(target.current_item(), QueueItem::Episode(8));
        assert!(
            youtube.snapshot().is_none(),
            "the completed session must not persist its old end position over mark_played"
        );
        assert!(super::automatic_completion_target(&direct_session(PodcastKind::Rss)).is_none());

        let mut queued_youtube = direct_session(PodcastKind::Youtube);
        queued_youtube.origin = PodcastOrigin::ManualQueue;
        assert!(super::automatic_completion_target(&queued_youtube).is_none());

        let mut final_youtube = direct_session(PodcastKind::Youtube);
        final_youtube.neighbours = NeighbourContext::for_episode(&[7], 7);
        assert!(super::automatic_completion_target(&final_youtube).is_none());
    }

    /// Marking an episode played lowers the unplayed counts behind the
    /// Podcasts and YouTube sidebar rows. Those counts are only recomputed by a
    /// full sidebar rebuild, and the queue-changed path deliberately no longer
    /// triggers one — it patches a single badge instead, which is what keeps a
    /// track change off the database. So every `mark_played` here has to
    /// announce itself, or the badge silently keeps the old number until some
    /// unrelated rebuild happens to fire.
    ///
    /// Checked against the source because the alternative needs a live
    /// `PlayerController` with a GTK window; the coupling this guards is
    /// exactly "these two calls stay together".
    #[test]
    fn every_mark_played_announces_the_changed_sidebar_count() {
        let source = include_str!("external_media_completion.rs");
        let played = source.matches("store::mark_played(").count();
        let announced = source.matches("self.notify_episode_played();").count();

        assert!(
            played > 0,
            "the completion paths must still mark episodes played"
        );
        assert_eq!(
            played, announced,
            "each of the {played} mark_played call(s) needs its own \
             notify_episode_played, found {announced}"
        );
    }
}
