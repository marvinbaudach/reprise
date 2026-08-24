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
            // Same reason as in `finish_queued_episode`: database-backed
            // sidebar counts and the source row's status just moved.
            self.notify_episode_played(episode_id);
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
            // The unplayed counts and the completed source row just changed.
            // The queue-changed path below only patches the Queue badge.
            self.notify_episode_played(episode_id);
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
            fallback_art_url: None,
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
        let completion = include_str!("external_media_completion.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        let position = include_str!("external_media_position.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        let played = completion.matches("store::mark_played(").count()
            + position.matches("store::mark_played(").count();
        let announced = completion
            .matches("self.notify_episode_played(episode_id);")
            .count()
            + position
                .matches("self.notify_episode_played(episode_id);")
                .count();

        assert!(
            position.contains("store::mark_played("),
            "leaving near the end must mark the episode played"
        );
        assert_eq!(
            played, announced,
            "each of the {played} mark_played call(s) needs its own \
             notify_episode_played, found {announced}"
        );
    }

    #[test]
    fn pausing_checkpoints_without_running_the_leaving_completion_decision() {
        let source = include_str!("external_media.rs");
        let pause = source
            .split("fn toggle_external_pause")
            .nth(1)
            .unwrap()
            .split("fn stop_external")
            .next()
            .unwrap();

        assert!(pause.contains("self.checkpoint_external_position();"));
        assert!(!pause.contains("self.persist_external_position();"));
    }

    #[test]
    fn a_new_short_duration_clears_the_stale_resume_position() {
        let source = include_str!("external_media_position.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();

        assert!(source.contains("save_position(&self.conn, episode_id, 0)"));
        assert!(source.contains("self.notify_episode_position(episode_id, 0);"));
    }

    #[test]
    fn completed_episode_ids_reach_both_source_views_and_the_sidebar() {
        let completion = include_str!("external_media_completion.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        let callbacks = include_str!("external_media.rs");
        let window = include_str!("../window/window.rs");
        let source_views = include_str!("../window/source_views.rs");

        assert_eq!(
            completion
                .matches("self.notify_episode_played(episode_id);")
                .count(),
            completion.matches("store::mark_played(").count()
        );
        assert!(callbacks.contains("callback: impl Fn(i64) + 'static"));
        assert!(callbacks.contains("fn notify_episode_played(&self, episode_id: i64)"));
        assert!(window.contains("source_views.wire_episode_played(player, &sidebar)"));
        assert!(source_views.contains("player.add_on_episode_played(move |episode_id|"));
        assert!(source_views.contains("sidebar.refresh(\"episode played\")"));
        assert_eq!(
            source_views
                .matches("update_played_state(episode_id)")
                .count(),
            1,
            "the Podcasts and YouTube views must both receive the completed ID"
        );
        assert!(source_views.contains("[self.podcasts.clone(), self.youtube.clone()]"));
        assert!(source_views.contains("page.if_materialized"));
    }

    #[test]
    fn persisted_episode_positions_reach_both_views_without_refreshing_the_sidebar() {
        let position = include_str!("external_media_position.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        let window = include_str!("../window/window.rs");
        let source = include_str!("../window/source_views.rs");
        let wiring = source
            .split("fn wire_episode_position")
            .nth(1)
            .unwrap()
            .split("fn set_toast_overlay")
            .next()
            .unwrap();

        assert!(position.contains("self.notify_episode_position(episode_id, position_ms);"));
        assert!(window.contains("source_views.wire_episode_position(player)"));
        assert!(wiring.contains("player.add_on_episode_position(move |episode_id, position_ms|"));
        assert_eq!(
            wiring
                .matches("update_position_state(episode_id, position_ms)")
                .count(),
            1
        );
        assert!(wiring.contains("[self.podcasts.clone(), self.youtube.clone()]"));
        assert!(wiring.contains("page.if_materialized"));
        assert!(!wiring.contains("sidebar.refresh"));
    }

    #[test]
    fn both_episode_surfaces_use_the_sparse_display_key_update_decision() {
        let marker = include_str!("../podcasts/podcasts_view_marker.rs");
        let detail = include_str!("../podcasts/youtube_channel_detail_status.rs");

        assert!(marker.contains("podcasts_presentation::update_resume_position(row, position_ms)"));
        assert!(marker.contains("if display_changed"));
        assert!(
            marker.contains("self.groups.borrow_mut()"),
            "a later full render must keep the patched position"
        );
        assert!(detail.contains("podcasts_presentation::update_resume_position(row, position_ms)"));
        assert!(detail.contains("if display_changed"));
    }
}
