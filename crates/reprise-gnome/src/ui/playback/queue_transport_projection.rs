use super::{compose_virtual, QueueItem, QueueViewModel, VirtualContextTail};
use crate::ui::playback::external_media_state::{
    ExternalMedia, ExternalPlaybackState, ExternalSession, PodcastOrigin,
};
use crate::ui::playback::preview::PlaybackMode;

pub(super) fn compose_queue_view_model(
    mode: PlaybackMode,
    queue_current: Option<i64>,
    current_up_next: Option<QueueItem>,
    play_next: &[QueueItem],
    music_context: Option<VirtualContextTail>,
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
            VirtualContextTail::materialised(
                neighbours.upcoming().to_vec(),
                (session.subscription_id as u64, neighbours.sequence),
                neighbours.position(),
            )
        })
    });
    compose_virtual(Some(now_playing), play_next, context, Some(show))
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
            published_at: None,
            art_url: None,
            phase: PodcastPhase::Playing,
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

    fn music_context() -> VirtualContextTail {
        VirtualContextTail::materialised(vec![QueueItem::Track(2), QueueItem::Track(3)], (5, 9), 1)
    }

    #[test]
    fn que_10_direct_episode_projects_frozen_show_context_not_music() {
        let external = podcast_state(PodcastOrigin::Direct, 7, Some(&[7, 8, 9]));

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
            model.all_items(),
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
        assert!(!model.all_items().contains(&QueueItem::Track(2)));
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

        assert_eq!(model.all_items(), vec![QueueItem::Episode(7)]);
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
        let after = compose_queue_view_model(
            PlaybackMode::Queue,
            Some(1),
            None,
            &[QueueItem::Track(90)],
            Some(music_context()),
            Some("Music"),
            &external,
        );

        assert_ne!(during.all_items(), before.all_items());
        assert_eq!(after, before);
        assert_eq!(after.all_items(), before.all_items());
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
            model.all_items(),
            vec![
                QueueItem::Episode(7),
                QueueItem::Track(90),
                QueueItem::Track(2),
                QueueItem::Track(3),
            ]
        );
    }
}
