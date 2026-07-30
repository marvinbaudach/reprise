//! MPRIS projection for podcast and live-radio sessions.

use reprise_core::media_integration::{MprisPlaybackStatus, MprisState};
use reprise_core::playback::PlaybackState;

use crate::ui::player_controller::PlayerController;

use super::external_media::{ExternalMedia, PodcastPhase};
use super::external_media_state::{ExternalSession, NeighbourContext, RadioPhase};
use super::preview::PlaybackMode;

struct ExternalMprisProjection {
    external_ref: String,
    live_stream: bool,
    title: String,
    artist: String,
    art_url: Option<String>,
    duration_ms: i64,
    position_ms: i64,
    can_next: bool,
    can_prev: bool,
}

impl PlayerController {
    pub(in crate::ui) fn external_state_changed(&self, state: PlaybackState) {
        let state = if self.playback_mode() == PlaybackMode::Radio {
            let phase = {
                let external = self.external.borrow();
                match external.session.as_ref() {
                    Some(ExternalSession::Radio(session)) => Some(session.presentation.phase),
                    _ => None,
                }
            };
            phase.map_or(state, |phase| project_radio_backend_state(phase, state))
        } else {
            state
        };
        if state == PlaybackState::Paused && self.radio_is_presented_paused() {
            self.sync_state(state);
            self.update_external_mpris(MprisPlaybackStatus::Paused);
            return;
        }
        self.sync_state(state);
        self.update_external_mpris(match state {
            PlaybackState::Playing => MprisPlaybackStatus::Playing,
            PlaybackState::Paused => MprisPlaybackStatus::Paused,
            PlaybackState::Stopped => MprisPlaybackStatus::Stopped,
        });
    }

    pub(in crate::ui) fn radio_is_presented_paused(&self) -> bool {
        let external = self.external.borrow();
        matches!(
            external.session.as_ref(),
            Some(ExternalSession::Radio(session))
                if session.presentation.phase == RadioPhase::Paused
        )
    }

    pub(super) fn update_external_mpris(&self, status: MprisPlaybackStatus) {
        let projection = {
            let external = self.external.borrow();
            match external.session.as_ref() {
                Some(ExternalSession::Podcast(session)) => {
                    let ExternalMedia::Podcast {
                        episode_id,
                        title,
                        show,
                        duration_ms,
                        ..
                    } = &session.media
                    else {
                        unreachable!("podcast session contains radio media")
                    };
                    ExternalMprisProjection {
                        external_ref: format!("podcast/{episode_id}"),
                        live_stream: false,
                        title: title.clone(),
                        artist: show.clone(),
                        art_url: session.art_url.clone(),
                        duration_ms: duration_ms.unwrap_or_default(),
                        position_ms: session.position_ms,
                        can_next: session
                            .neighbours
                            .as_ref()
                            .is_some_and(NeighbourContext::has_next),
                        can_prev: session
                            .neighbours
                            .as_ref()
                            .is_some_and(NeighbourContext::has_previous),
                    }
                }
                Some(ExternalSession::Radio(session)) => {
                    let ExternalMedia::Radio {
                        station_id, name, ..
                    } = &session.media
                    else {
                        unreachable!("radio session contains podcast media")
                    };
                    ExternalMprisProjection {
                        external_ref: format!("radio/{station_id}"),
                        live_stream: true,
                        title: session
                            .presentation
                            .last_title
                            .clone()
                            .unwrap_or_else(|| name.clone()),
                        artist: name.clone(),
                        art_url: session.art_url.clone(),
                        duration_ms: 0,
                        position_ms: 0,
                        can_next: false,
                        can_prev: false,
                    }
                }
                None => return,
            }
        };
        let mut mirror = self
            .mpris_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let position_ms =
            if mirror.external_ref.as_deref() == Some(projection.external_ref.as_str()) {
                mirror.position_ms
            } else {
                projection.position_ms
            };
        *mirror = MprisState {
            status,
            track_id: None,
            external_ref: Some(projection.external_ref),
            live_stream: projection.live_stream,
            title: projection.title,
            artist: projection.artist,
            album: String::new(),
            art_url: projection.art_url,
            duration_ms: projection.duration_ms,
            can_next: projection.can_next,
            can_prev: projection.can_prev,
            position_ms,
            shuffle: self.queue.borrow().is_shuffled(),
            repeat: self.queue.borrow().repeat(),
            volume: self.volume.get(),
        };
    }

    pub(super) fn external_mpris_status(&self) -> MprisPlaybackStatus {
        let external = self.external.borrow();
        match external.session.as_ref() {
            Some(ExternalSession::Podcast(session)) => match session.phase {
                PodcastPhase::Playing | PodcastPhase::Resolving => MprisPlaybackStatus::Playing,
                PodcastPhase::Paused => MprisPlaybackStatus::Paused,
                PodcastPhase::Failed => MprisPlaybackStatus::Stopped,
            },
            Some(ExternalSession::Radio(session))
                if session.presentation.phase != RadioPhase::Connected =>
            {
                MprisPlaybackStatus::Paused
            }
            Some(ExternalSession::Radio(_)) => MprisPlaybackStatus::Playing,
            None => MprisPlaybackStatus::Stopped,
        }
    }

    pub(super) fn update_podcast_duration(
        &self,
        generation: u64,
        episode_id: i64,
        duration_ms: i64,
    ) {
        {
            let mut external = self.external.borrow_mut();
            if external.generation != generation {
                return;
            }
            let Some(ExternalSession::Podcast(session)) = external.session.as_mut() else {
                return;
            };
            let ExternalMedia::Podcast {
                episode_id: current_id,
                duration_ms: current,
                ..
            } = &mut session.media
            else {
                return;
            };
            if *current_id != episode_id {
                return;
            }
            *current = Some(duration_ms);
            session.duration_known = true;
        }
        self.update_external_mpris(self.external_mpris_status());
    }
}

fn project_radio_backend_state(phase: RadioPhase, backend: PlaybackState) -> PlaybackState {
    match phase {
        RadioPhase::Paused => PlaybackState::Paused,
        RadioPhase::Reconnecting => PlaybackState::Playing,
        RadioPhase::Connected => backend,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paused_and_reconnecting_radio_ignore_stale_backend_state() {
        assert_eq!(
            project_radio_backend_state(RadioPhase::Paused, PlaybackState::Playing),
            PlaybackState::Paused
        );
        assert_eq!(
            project_radio_backend_state(RadioPhase::Reconnecting, PlaybackState::Stopped),
            PlaybackState::Playing
        );
        assert_eq!(
            project_radio_backend_state(RadioPhase::Connected, PlaybackState::Stopped),
            PlaybackState::Stopped
        );
    }
}
