//! Cold-start persistence for podcast and YouTube playback sessions.

use std::rc::Rc;

use reprise_core::library::session::{SessionEpisode, SessionEpisodeOrigin};
use reprise_core::media_integration::MprisPlaybackStatus;
use reprise_core::playback::PlaybackState;
use reprise_core::podcasts::EpisodeRow;
use reprise_core::up_next::QueueItem;

use crate::ui::player_controller::PlayerController;

use super::external_media::media_from_episode;
use super::external_media_state::{
    episode_artwork_urls, ExternalMedia, ExternalPlaybackState, ExternalSession, NeighbourContext,
    PodcastOrigin, PodcastPhase, PodcastSession, ResumePolicy,
};

type RestoredResumeRequest = (ExternalMedia, Option<NeighbourContext>, PodcastOrigin);

fn restored_session(
    saved: &SessionEpisode,
    episode: &EpisodeRow,
    manual_pending: &[QueueItem],
) -> Option<PodcastSession> {
    if saved.episode_id != episode.id {
        return None;
    }
    let (origin, neighbours) = match saved.origin {
        SessionEpisodeOrigin::Direct => (
            PodcastOrigin::Direct,
            NeighbourContext::for_episode(&saved.neighbour_episode_ids, episode.id),
        ),
        SessionEpisodeOrigin::ManualQueue => (
            PodcastOrigin::ManualQueue,
            NeighbourContext::for_manual_queue(QueueItem::Episode(episode.id), manual_pending),
        ),
    };
    let position_ms = episode.position_ms.max(0);
    let (art_url, fallback_art_url) = episode_artwork_urls(episode);
    Some(PodcastSession {
        media: media_from_episode(episode),
        neighbours,
        automatic_advance: None,
        subscription_id: episode.subscription_id,
        kind: episode.kind,
        media_category: episode.media_category.clone(),
        published_at: episode.published_at,
        art_url,
        fallback_art_url,
        phase: PodcastPhase::Paused,
        restored: true,
        origin,
        resume: ResumePolicy::new(position_ms),
        position_ms,
        last_persisted_ms: position_ms,
        duration_known: episode.duration_secs.is_some_and(|duration| duration > 0),
        error: None,
    })
}

fn restored_resume_request(state: &ExternalPlaybackState) -> Option<RestoredResumeRequest> {
    let ExternalSession::Podcast(session) = state.session.as_ref()? else {
        return None;
    };
    session.restored.then(|| {
        (
            session.media.clone(),
            session.neighbours.clone(),
            session.origin,
        )
    })
}

impl ExternalPlaybackState {
    pub(in crate::ui) fn session_episode(&self) -> Option<SessionEpisode> {
        let ExternalSession::Podcast(session) = self.session.as_ref()? else {
            return None;
        };
        let ExternalMedia::Podcast { episode_id, .. } = session.media else {
            return None;
        };
        let origin = match session.origin {
            PodcastOrigin::Direct => SessionEpisodeOrigin::Direct,
            PodcastOrigin::ManualQueue => SessionEpisodeOrigin::ManualQueue,
        };
        let neighbour_episode_ids = if session.origin == PodcastOrigin::Direct {
            session
                .neighbours
                .as_ref()
                .and_then(NeighbourContext::episode_ids)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        Some(SessionEpisode {
            episode_id,
            origin,
            neighbour_episode_ids,
        })
    }
}

impl PlayerController {
    pub(in crate::ui) fn session_episode_snapshot(&self) -> Option<SessionEpisode> {
        self.external.borrow().session_episode()
    }

    /// Reconstructs paused metadata only. No stale stream URL or backend
    /// pipeline crosses a process boundary; the first Play resolves the
    /// durable episode row through the normal playback path.
    pub(in crate::ui) fn restore_session_episode(&self, saved: Option<&SessionEpisode>) -> bool {
        let Some(saved) = saved else {
            return false;
        };
        let episode = match reprise_core::podcasts::store::episode(&self.conn, saved.episode_id) {
            Ok(Some(episode)) => episode,
            Ok(None) => {
                tracing::info!(
                    episode_id = saved.episode_id,
                    "restored episode is no longer available"
                );
                return false;
            }
            Err(error) => {
                tracing::warn!(
                    %error,
                    episode_id = saved.episode_id,
                    "could not restore episode session"
                );
                return false;
            }
        };
        let manual_pending = self.up_next.borrow().ids().to_vec();
        let Some(session) = restored_session(saved, &episode, &manual_pending) else {
            return false;
        };
        let (title, show, position_ms) = match &session.media {
            ExternalMedia::Podcast {
                title,
                show,
                resume_ms,
                ..
            } => (title.clone(), show.clone(), (*resume_ms).max(0)),
            ExternalMedia::Radio { .. } => unreachable!("restored episode contains radio media"),
        };

        self.sync_lyrics_track(None);
        self.current_track.set(None);
        self.max_position_ms.set(0);
        self.player.set_next(None);
        *self.now_playing.borrow_mut() = None;
        self.external
            .borrow_mut()
            .begin_session(ExternalSession::Podcast(session));
        self.sync_track(&title, &show, "", None);
        self.sync_cover("");
        self.sync_state(PlaybackState::Paused);
        self.update_mpris_position(position_ms);
        self.update_external_mpris(MprisPlaybackStatus::Paused);
        self.notify_external_changed();
        tracing::info!(
            episode_id = saved.episode_id,
            position_ms,
            "episode session restored paused"
        );
        true
    }

    pub(super) fn resume_restored_episode(self: &Rc<Self>) -> bool {
        let request = {
            let external = self.external.borrow();
            restored_resume_request(&external)
        };
        let Some((media, neighbours, origin)) = request else {
            return false;
        };
        if let Err(error) =
            self.play_external_with_context_and_origin(media, neighbours, None, origin)
        {
            tracing::error!(%error, "restored episode playback failed");
            self.show_toast(&error.to_string());
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::library::session::SessionEpisodeOrigin;
    use reprise_core::podcasts::PodcastKind;

    fn episode() -> EpisodeRow {
        EpisodeRow {
            id: 7,
            subscription_id: 2,
            guid: "episode-7".into(),
            title: "Last episode".into(),
            show: "VOID PREACHER".into(),
            show_image_url: Some("https://images.test/show.jpg".into()),
            image_url: None,
            kind: PodcastKind::Youtube,
            audio_url: "https://youtube.test/watch?v=7".into(),
            page_url: None,
            published_at: Some(20),
            duration_secs: Some(3_900),
            downloaded_path: None,
            downloaded_bytes: None,
            played_at: None,
            position_ms: 22_000,
            first_seen_at: 10,
            is_new: false,
            media_category: Some("Music".into()),
        }
    }

    #[test]
    fn que_11_restores_a_direct_episode_as_a_paused_metadata_session() {
        let saved = SessionEpisode {
            episode_id: 7,
            origin: SessionEpisodeOrigin::Direct,
            neighbour_episode_ids: vec![6, 7, 8],
        };

        let session = restored_session(&saved, &episode(), &[]).unwrap();

        assert!(session.restored);
        assert_eq!(
            session.phase,
            super::super::external_media_state::PodcastPhase::Paused
        );
        assert_eq!(session.position_ms, 22_000);
        assert_eq!(session.media_category.as_deref(), Some("Music"));
        assert_eq!(
            session.art_url.as_deref(),
            Some("https://images.test/show.jpg")
        );
        assert_eq!(session.neighbours.as_ref().unwrap().current_id(), 7);

        let state = ExternalPlaybackState {
            session: Some(ExternalSession::Podcast(session)),
            ..ExternalPlaybackState::default()
        };
        let snapshot = state.snapshot().unwrap();
        assert!(snapshot.restored);
        assert_eq!(snapshot.podcast_phase, Some(PodcastPhase::Paused));
    }

    #[test]
    fn src_11_source_image_restore_keeps_episode_then_show_artwork() {
        let saved = SessionEpisode {
            episode_id: 7,
            origin: SessionEpisodeOrigin::Direct,
            neighbour_episode_ids: vec![7],
        };
        let mut episode = episode();
        episode.image_url = Some("https://images.test/episode.jpg".into());

        let session = restored_session(&saved, &episode, &[]).unwrap();
        assert_eq!(
            session.art_url.as_deref(),
            Some("https://images.test/episode.jpg")
        );
        assert_eq!(
            session.fallback_art_url.as_deref(),
            Some("https://images.test/show.jpg")
        );

        let state = ExternalPlaybackState {
            session: Some(ExternalSession::Podcast(session)),
            ..ExternalPlaybackState::default()
        };
        let snapshot = state.snapshot().unwrap();
        assert_eq!(
            snapshot.fallback_art_url.as_deref(),
            Some("https://images.test/show.jpg")
        );
    }

    #[test]
    fn que_11_direct_episode_projects_a_stable_session_identity() {
        let saved = SessionEpisode {
            episode_id: 7,
            origin: SessionEpisodeOrigin::Direct,
            neighbour_episode_ids: vec![6, 7, 8],
        };
        let session = restored_session(&saved, &episode(), &[]).unwrap();
        let state = ExternalPlaybackState {
            session: Some(ExternalSession::Podcast(session)),
            ..ExternalPlaybackState::default()
        };

        assert_eq!(state.session_episode(), Some(saved));
    }

    #[test]
    fn start_3_first_play_reopens_only_a_restored_episode() {
        let saved = SessionEpisode {
            episode_id: 7,
            origin: SessionEpisodeOrigin::Direct,
            neighbour_episode_ids: vec![6, 7, 8],
        };
        let session = restored_session(&saved, &episode(), &[]).unwrap();
        let mut state = super::super::external_media_state::ExternalPlaybackState::default();
        state.begin_session(super::super::external_media_state::ExternalSession::Podcast(session));

        let request = restored_resume_request(&state).unwrap();
        assert_eq!(request.0, media_from_episode(&episode()));
        assert_eq!(request.1.unwrap().current_id(), 7);
        assert_eq!(request.2, PodcastOrigin::Direct);

        let Some(super::super::external_media_state::ExternalSession::Podcast(session)) =
            state.session.as_mut()
        else {
            panic!("expected podcast session");
        };
        session.restored = false;
        assert!(restored_resume_request(&state).is_none());
    }
}
