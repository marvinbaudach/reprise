use std::rc::Rc;

use crate::ui::playback::external_media_state::{
    ExternalMedia, ExternalPlaybackState, ExternalSession, PodcastOrigin,
};
use crate::ui::playback::preview::PlaybackMode;
use crate::ui::player_controller::PlayerController;
use crate::ui::track_list::queue_sections::{compose_virtual, QueueViewModel, VirtualContext};
use reprise_core::up_next::QueueItem;

pub(super) fn compose_queue_view_model(
    mode: PlaybackMode,
    queue_current: Option<i64>,
    current_up_next: Option<QueueItem>,
    play_next: &[QueueItem],
    music_context: Option<VirtualContext>,
    music_origin_label: Option<&str>,
    external: &ExternalPlaybackState,
) -> QueueViewModel {
    match mode {
        PlaybackMode::Queue => compose_virtual(
            current_up_next.or(queue_current.map(QueueItem::Track)),
            play_next,
            music_context,
            music_origin_label,
        ),
        PlaybackMode::QueuedEpisode => compose_virtual(
            current_up_next,
            play_next,
            music_context,
            music_origin_label,
        ),
        PlaybackMode::Podcast => direct_podcast_model(play_next, external),
        PlaybackMode::Preview | PlaybackMode::Radio => compose_virtual(None, play_next, None, None),
    }
}

fn direct_podcast_model(
    play_next: &[QueueItem],
    external: &ExternalPlaybackState,
) -> QueueViewModel {
    let Some(ExternalSession::Podcast(session)) = external.session.as_ref() else {
        return compose_virtual(None, play_next, None, None);
    };
    if session.origin != PodcastOrigin::Direct {
        return compose_virtual(None, play_next, None, None);
    }
    let ExternalMedia::Podcast {
        episode_id, show, ..
    } = &session.media
    else {
        return compose_virtual(None, play_next, None, None);
    };
    let now_playing = session
        .neighbours
        .as_ref()
        .map_or(QueueItem::Episode(*episode_id), |context| {
            context.current_item()
        });
    let context = session.neighbours.as_ref().and_then(|neighbours| {
        (!neighbours.upcoming().is_empty()).then(|| {
            VirtualContext::identified(
                neighbours.upcoming().len(),
                (session.subscription_id as u64, neighbours.sequence),
                neighbours.position(),
            )
        })
    });
    compose_virtual(Some(now_playing), play_next, context, Some(show))
}

pub(super) fn has_direct_episode_projection(external: &ExternalPlaybackState) -> bool {
    matches!(
        external.session.as_ref(),
        Some(ExternalSession::Podcast(session)) if session.origin == PodcastOrigin::Direct
    )
}

impl PlayerController {
    /// The Queue view's three parts in display order (QUE-1): the playing
    /// item, pending manual entries, and virtual context tail.
    pub(in crate::ui) fn queue_view_model(self: &Rc<Self>) -> QueueViewModel {
        let deferred = self.deferred_queue_purge_id.get();
        let mode = self.playback_mode();
        let current_up_next = self
            .current_up_next
            .get()
            .filter(|item| item.track_id().is_none_or(|id| Some(id) != deferred));
        let queue_current = self
            .queue
            .borrow()
            .current()
            .filter(|id| Some(*id) != deferred);
        let play_next = self.up_next.borrow().ids().to_vec();
        let (context_count, context_sequence, context_start) = {
            let queue = self.queue.borrow();
            (
                queue.remaining_len(),
                queue.sequence_identity(),
                queue
                    .current_order_position()
                    .map_or(0, |position| position + 1),
            )
        };
        let origin_label = self
            .play_origin
            .borrow()
            .as_ref()
            .map(|origin| origin.label.clone());
        let context = (context_count > 0)
            .then(|| VirtualContext::identified(context_count, context_sequence, context_start));
        compose_queue_view_model(
            mode,
            queue_current,
            current_up_next,
            &play_next,
            context,
            origin_label.as_deref(),
            &self.external.borrow(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::playback::external_media_state::{
        EpisodeSource, ExternalMedia, ExternalPlaybackState, ExternalSession, NeighbourContext,
        PodcastOrigin, PodcastPhase, PodcastSession, ResumePolicy,
    };
    use crate::ui::track_list::queue_sections::QueueSectionKind;

    fn podcast_state(
        origin: PodcastOrigin,
        episode_id: i64,
        neighbour_ids: Option<&[i64]>,
    ) -> ExternalPlaybackState {
        let neighbours =
            neighbour_ids.and_then(|ids| NeighbourContext::for_episode(ids, episode_id));
        let session = PodcastSession {
            media: ExternalMedia::Podcast {
                episode_id,
                title: format!("Episode {episode_id}"),
                show: "VOID PREACHER".into(),
                source: EpisodeSource::Url("https://example.test/episode.mp3".into()),
                resume_ms: 0,
                duration_ms: None,
            },
            neighbours,
            automatic_advance: None,
            subscription_id: 42,
            kind: reprise_core::podcasts::PodcastKind::Rss,
            media_category: None,
            published_at: None,
            art_url: None,
            fallback_art_url: None,
            phase: PodcastPhase::Playing,
            restored: false,
            origin,
            resume: ResumePolicy::new(0),
            position_ms: 0,
            last_persisted_ms: 0,
            duration_known: false,
            error: None,
        };
        ExternalPlaybackState {
            session: Some(ExternalSession::Podcast(session)),
            generation: 77,
            ..ExternalPlaybackState::default()
        }
    }

    fn music_context() -> VirtualContext {
        VirtualContext::identified(2, (5, 9), 1)
    }

    fn music_context_items() -> Vec<QueueItem> {
        vec![QueueItem::Track(2), QueueItem::Track(3)]
    }

    #[test]
    fn que_10_direct_episode_projects_frozen_show_context_not_music() {
        let external = podcast_state(PodcastOrigin::Direct, 7, Some(&[7, 8, 9]));
        let episode_context = vec![QueueItem::Episode(8), QueueItem::Episode(9)];

        let model = compose_queue_view_model(
            PlaybackMode::Podcast,
            Some(1),
            None,
            &[QueueItem::Track(90)],
            Some(music_context()),
            Some("Music"),
            &external,
        );

        assert_eq!(
            model.all_items(&episode_context),
            vec![
                QueueItem::Episode(7),
                QueueItem::Track(90),
                QueueItem::Episode(8),
                QueueItem::Episode(9),
            ]
        );
        assert_eq!(model.sidebar_count(), 1);
        assert_eq!(
            model.sections.last().map(|section| &section.kind),
            Some(&QueueSectionKind::UpNext {
                source_label: "VOID PREACHER".into(),
            })
        );
        assert!(!model
            .all_items(&episode_context)
            .contains(&QueueItem::Track(2)));
    }

    #[test]
    fn direct_episode_without_neighbours_still_projects_now_playing() {
        let external = podcast_state(PodcastOrigin::Direct, 7, None);

        let model = compose_queue_view_model(
            PlaybackMode::Podcast,
            Some(1),
            None,
            &[],
            Some(music_context()),
            Some("Music"),
            &external,
        );

        assert_eq!(
            model.all_items(&Vec::<QueueItem>::new()),
            vec![QueueItem::Episode(7)]
        );
        assert_eq!(model.sections.len(), 1);
        assert_eq!(model.sections[0].kind, QueueSectionKind::NowPlaying);
    }

    #[test]
    fn returning_to_queue_restores_the_unchanged_music_projection() {
        let external = ExternalPlaybackState::default();
        let before = compose_queue_view_model(
            PlaybackMode::Queue,
            Some(1),
            None,
            &[QueueItem::Track(90)],
            Some(music_context()),
            Some("Music"),
            &external,
        );
        let during = compose_queue_view_model(
            PlaybackMode::Podcast,
            Some(1),
            None,
            &[QueueItem::Track(90)],
            Some(music_context()),
            Some("Music"),
            &podcast_state(PodcastOrigin::Direct, 7, Some(&[7, 8, 9])),
        );
        let episode_context = vec![QueueItem::Episode(8), QueueItem::Episode(9)];
        let after = compose_queue_view_model(
            PlaybackMode::Queue,
            Some(1),
            None,
            &[QueueItem::Track(90)],
            Some(music_context()),
            Some("Music"),
            &external,
        );

        assert_ne!(
            during.all_items(&episode_context),
            before.all_items(&music_context_items())
        );
        assert_eq!(after, before);
        assert_eq!(
            after.all_items(&music_context_items()),
            before.all_items(&music_context_items())
        );
    }

    #[test]
    fn queued_episode_keeps_the_manual_queue_projection() {
        let external = podcast_state(PodcastOrigin::ManualQueue, 7, Some(&[7, 90]));

        let model = compose_queue_view_model(
            PlaybackMode::QueuedEpisode,
            Some(1),
            Some(QueueItem::Episode(7)),
            &[QueueItem::Track(90)],
            Some(music_context()),
            Some("Music"),
            &external,
        );

        assert_eq!(
            model.all_items(&music_context_items()),
            vec![
                QueueItem::Episode(7),
                QueueItem::Track(90),
                QueueItem::Track(2),
                QueueItem::Track(3),
            ]
        );
    }

    #[test]
    fn only_a_direct_podcast_session_has_a_read_only_episode_projection() {
        assert!(has_direct_episode_projection(&podcast_state(
            PodcastOrigin::Direct,
            7,
            Some(&[7, 8])
        )));
        assert!(!has_direct_episode_projection(&podcast_state(
            PodcastOrigin::ManualQueue,
            7,
            Some(&[7, 8])
        )));
        assert!(!has_direct_episode_projection(
            &ExternalPlaybackState::default()
        ));
    }
}
