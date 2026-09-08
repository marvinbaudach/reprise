use std::rc::Rc;

use crate::ui::playback::external_media_state::{
    ExternalMedia, ExternalPlaybackState, ExternalSession, PodcastOrigin,
};
use crate::ui::playback::preview::PlaybackMode;
use crate::ui::player_controller::PlayerController;
use crate::ui::track_list::queue_sections::{compose_virtual, QueueViewModel, VirtualContext};
use reprise_core::up_next::QueueItem;
use reprise_view::queue::{LastTail, TailChange};

fn tail_change(old: &LastTail, new: &LastTail) -> Option<TailChange> {
    let prefix = old
        .ids
        .iter()
        .zip(&new.ids)
        .take_while(|(old, new)| old == new)
        .count();
    let suffix = old.ids[prefix..]
        .iter()
        .rev()
        .zip(new.ids[prefix..].iter().rev())
        .take_while(|(old, new)| old == new)
        .count();
    let removed = old.ids.len() - prefix - suffix;
    let added = new.ids.len() - prefix - suffix;
    (removed != 0 || added != 0).then_some(TailChange {
        base: old.sequence,
        base_start: old.start,
        position: prefix,
        removed,
        added,
    })
}

fn tail_context(old: Option<&LastTail>, new: &LastTail) -> Option<VirtualContext> {
    let change = old.and_then(|old| tail_change(old, new));
    if new.ids.is_empty() && change.is_none() {
        return None;
    }
    Some(VirtualContext::identified_with_change(
        new.ids.len(),
        new.sequence,
        new.start,
        change,
    ))
}

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
        let tail = {
            let queue = self.queue.borrow();
            let count = queue.remaining_len();
            LastTail {
                sequence: queue.sequence_identity(),
                start: queue
                    .current_order_position()
                    .map_or(0, |position| position + 1),
                ids: queue.remaining_window(0, count),
            }
        };
        let origin_label = self
            .play_origin
            .borrow()
            .as_ref()
            .map(|origin| origin.label.clone());
        let previous_tail = self.last_composed_tail.borrow().clone();
        let change = previous_tail
            .as_ref()
            .and_then(|previous| tail_change(previous, &tail));
        let projects_music_context =
            matches!(mode, PlaybackMode::Queue | PlaybackMode::QueuedEpisode);
        let context = projects_music_context
            .then(|| tail_context(previous_tail.as_ref(), &tail))
            .flatten();
        if let Some(change) = change.filter(|_| projects_music_context) {
            tracing::info!(
                position = change.position,
                removed = change.removed,
                added = change.added,
                tail_len = tail.ids.len(),
                "queue tail change"
            );
        }
        let model = compose_queue_view_model(
            mode,
            queue_current,
            current_up_next,
            &play_next,
            context,
            origin_label.as_deref(),
            &self.external.borrow(),
        );
        self.last_composed_tail.replace(Some(tail));
        model
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

    fn tail(sequence: (u64, u64), start: usize, ids: &[i64]) -> LastTail {
        LastTail {
            sequence,
            start,
            ids: ids.to_vec(),
        }
    }

    #[test]
    fn queue_tail_change_covers_a_middle_removal() {
        let old = tail((4, 1), 8, &[10, 11, 12, 13]);
        let new = tail((4, 2), 8, &[10, 13]);

        assert_eq!(
            tail_context(Some(&old), &new),
            Some(VirtualContext::identified_with_change(
                2,
                (4, 2),
                8,
                Some(TailChange {
                    base: (4, 1),
                    base_start: 8,
                    position: 1,
                    removed: 2,
                    added: 0,
                })
            ))
        );
    }

    #[test]
    fn queue_tail_change_covers_two_separated_removals() {
        let old = tail((4, 1), 8, &[10, 11, 12, 13, 14]);
        let new = tail((4, 2), 8, &[10, 12, 14]);

        assert_eq!(
            tail_change(&old, &new),
            Some(TailChange {
                base: (4, 1),
                base_start: 8,
                position: 1,
                removed: 3,
                added: 1,
            })
        );
    }

    #[test]
    fn queue_tail_change_recognises_removing_the_current_track() {
        let old = tail((4, 1), 8, &[10, 11, 12]);
        let new = tail((4, 2), 8, &[11, 12]);

        assert_eq!(
            tail_change(&old, &new),
            Some(TailChange {
                base: (4, 1),
                base_start: 8,
                position: 0,
                removed: 1,
                added: 0,
            })
        );
    }

    #[test]
    fn first_queue_tail_projection_has_no_change_hint() {
        let new = tail((4, 1), 8, &[10, 11]);

        assert_eq!(
            tail_context(None, &new),
            Some(VirtualContext::identified(2, (4, 1), 8))
        );
    }

    #[test]
    fn unchanged_queue_tail_projection_has_no_change_hint() {
        let old = tail((4, 1), 8, &[10, 11]);
        let new = old.clone();

        assert_eq!(
            tail_context(Some(&old), &new),
            Some(VirtualContext::identified(2, (4, 1), 8))
        );
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
