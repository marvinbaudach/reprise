//! Position tracking and resume persistence for an external podcast session.
//!
//! Split out of `external_media.rs` to keep that file under the architecture
//! gate's 800-line limit. The whole of this module is one concern: what a
//! position tick from the pipeline means for the episode currently on screen.

use crate::ui::player_controller::PlayerController;
use reprise_core::podcasts::resume_rules;
use reprise_core::podcasts::PodcastKind;

use super::external_media::{session_id, POSITION_PERSIST_INTERVAL_MS};
use super::external_media_state::{ExternalMedia, ExternalSession, PodcastPhase};

impl PlayerController {
    pub(in crate::ui) fn persist_external_on_quit(&self) {
        self.persist_external_position();
    }

    pub(in crate::ui) fn handle_external_position(&self, position_ms: i64, duration_ms: i64) {
        let (episode_id, kind, position_ms, persist, save_duration, duration_secs, retry_seek) = {
            let mut external = self.external.borrow_mut();
            let Some(ExternalSession::Podcast(session)) = external.session.as_mut() else {
                return;
            };
            // A session that is still resolving has not produced a single
            // sample, so any tick arriving now belongs to the episode that was
            // playing before the switch — the pipeline keeps emitting them for
            // a moment. Attributing that position to the new episode would
            // persist a wrong resume point for it and, worse, mark it as
            // "genuinely playing" so a subsequent resolve failure would strand
            // the user instead of skipping on.
            if session.phase != PodcastPhase::Playing {
                return;
            }
            session.position_ms = position_ms.max(0);
            session.note_playback_progress(session.position_ms);
            let episode_id = session_id(&session.media);
            let persist = (session.position_ms - session.last_persisted_ms).abs()
                >= POSITION_PERSIST_INTERVAL_MS;
            if persist {
                session.last_persisted_ms = session.position_ms;
            }
            let save_duration = (!session.duration_known && duration_ms > 0).then_some(duration_ms);
            if save_duration.is_some() {
                session.duration_known = true;
                if let ExternalMedia::Podcast {
                    duration_ms: current,
                    ..
                } = &mut session.media
                {
                    *current = save_duration;
                }
            }
            let retry_seek = session.resume.position_tick(duration_ms);
            let duration_secs = podcast_duration_secs(&session.media);
            (
                episode_id,
                session.kind,
                session.position_ms,
                persist,
                save_duration,
                duration_secs,
                retry_seek,
            )
        };
        if persist && resume_rules::keeps_resume(kind, duration_secs) {
            if let Err(error) =
                reprise_core::podcasts::store::save_position(&self.conn, episode_id, position_ms)
            {
                tracing::warn!(%error, episode_id, "could not persist podcast position tick");
            } else {
                self.notify_episode_position(episode_id, position_ms);
            }
        }
        if let Some(duration_ms) = save_duration {
            let _ = reprise_core::podcasts::store::save_duration(
                &self.conn,
                episode_id,
                duration_ms / 1_000,
            );
            if !resume_rules::keeps_resume(kind, duration_secs) {
                if let Err(error) =
                    reprise_core::podcasts::store::save_position(&self.conn, episode_id, 0)
                {
                    tracing::warn!(%error, episode_id, "could not clear short podcast position");
                } else {
                    self.notify_episode_position(episode_id, 0);
                }
            }
            self.update_external_mpris(self.external_mpris_status());
        }
        if let Some(resume_ms) = retry_seek {
            let _ = self.player.seek_to(resume_ms);
        }
    }

    pub(super) fn checkpoint_external_position(&self) {
        let Some((episode_id, kind, position_ms, duration_secs)) = self.external_position() else {
            return;
        };
        if !resume_rules::keeps_resume(kind, duration_secs) {
            return;
        }
        if let Err(error) =
            reprise_core::podcasts::store::save_position(&self.conn, episode_id, position_ms)
        {
            tracing::warn!(%error, episode_id, "could not checkpoint podcast position");
        }
    }

    pub(super) fn persist_external_position(&self) {
        let Some((episode_id, kind, position_ms, duration_secs)) = self.external_position() else {
            return;
        };
        if resume_rules::is_complete(position_ms, duration_secs) {
            let now = chrono::Utc::now().timestamp();
            if let Err(error) =
                reprise_core::podcasts::store::mark_played(&self.conn, episode_id, now)
            {
                tracing::warn!(%error, episode_id, "could not mark leaving podcast played");
            } else {
                self.notify_episode_played(episode_id);
            }
            return;
        }
        if !resume_rules::keeps_resume(kind, duration_secs) {
            return;
        }
        if let Err(error) =
            reprise_core::podcasts::store::save_position(&self.conn, episode_id, position_ms)
        {
            tracing::warn!(%error, episode_id, "could not persist podcast position");
        }
    }

    fn external_position(&self) -> Option<(i64, PodcastKind, i64, Option<i64>)> {
        let external = self.external.borrow();
        let ExternalSession::Podcast(session) = external.session.as_ref()? else {
            return None;
        };
        Some((
            session_id(&session.media),
            session.kind,
            session.position_ms,
            podcast_duration_secs(&session.media),
        ))
    }
}

fn podcast_duration_secs(media: &ExternalMedia) -> Option<i64> {
    match media {
        ExternalMedia::Podcast { duration_ms, .. } => duration_ms.map(|duration| duration / 1_000),
        ExternalMedia::Radio { .. } => None,
    }
}
