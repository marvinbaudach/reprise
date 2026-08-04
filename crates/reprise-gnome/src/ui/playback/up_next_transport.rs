use reprise_core::library::settings::{self, TrackTransition};
use reprise_core::queries;
use reprise_core::queue::{Queue, Repeat};
use reprise_core::up_next::{QueueItem, UpNextQueue};

use crate::ui::player_controller::{PlayerController, StartPlayback};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum AdvanceReason {
    Automatic,
    Manual,
}

fn random_library_continuation(
    origin: Option<&super::play_origin::PlayOrigin>,
    visible_ids: &[i64],
    mut random_live_ids: Vec<i64>,
    finished_track_id: Option<i64>,
) -> Option<Vec<i64>> {
    let origin = origin?;
    if !origin.place.is_library_root() {
        return None;
    }
    let state = origin.place.track_state()?;
    if state.search.trim().is_empty() && state.browse.is_empty() {
        return None;
    }

    let visible = visible_ids
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    let live = random_live_ids
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    if visible.len() != visible_ids.len() || live.len() != random_live_ids.len() || visible != live
    {
        return None;
    }

    let finished_track_id = finished_track_id?;
    let next_position = random_live_ids
        .iter()
        .position(|id| *id != finished_track_id)?;
    random_live_ids.rotate_left(next_position);
    Some(random_live_ids)
}

fn drop_unavailable_episodes(
    pending: &mut UpNextQueue,
    available: &std::collections::HashSet<i64>,
) -> usize {
    let unavailable = pending
        .ids()
        .iter()
        .copied()
        .filter(|item| item.episode_id().is_some_and(|id| !available.contains(&id)))
        .collect::<Vec<_>>();
    pending.remove_ids(&unavailable)
}

fn next_matching_target(
    context: &mut Queue,
    pending: &mut UpNextQueue,
    current_pending: &mut Option<QueueItem>,
    reason: AdvanceReason,
    mut is_available: impl FnMut(QueueItem) -> bool,
) -> Option<QueueItem> {
    if reason == AdvanceReason::Automatic && context.repeat() == Repeat::One {
        if let Some(current) = current_pending.or_else(|| context.current().map(QueueItem::Track)) {
            if is_available(current) {
                return Some(current);
            }
        }
    }

    if let Some(next) = pending.take_first_matching(&mut is_available) {
        *current_pending = Some(next);
        return Some(next);
    }

    *current_pending = None;
    match reason {
        AdvanceReason::Automatic => {
            context.advance_auto_matching(|id| is_available(QueueItem::Track(id)))
        }
        AdvanceReason::Manual => {
            context.next_manual_matching(|id| is_available(QueueItem::Track(id)))
        }
    }
    .map(QueueItem::Track)
}

#[cfg(test)]
fn next_target(
    context: &mut Queue,
    pending: &mut UpNextQueue,
    current_pending: &mut Option<QueueItem>,
    reason: AdvanceReason,
) -> Option<QueueItem> {
    next_matching_target(context, pending, current_pending, reason, |_| true)
}

/// Non-mutating preview of the track an `Automatic` advance would select next,
/// for the gapless pre-feed (`feed_next`). Mirrors `next_target`'s `Automatic`
/// branch WITHOUT mutating the queue or up-next.
///
/// Repeat-One is deliberately excluded (returns `None`): looping the same track
/// keeps running through the ordinary end-of-stream path so play-tracking stays
/// a single, well-understood flow and we avoid a same-URI `about-to-finish`
/// edge case. The tiny gap on a repeat-one loop is an accepted trade-off.
fn peek_matching_auto_target(
    context: &Queue,
    pending: &UpNextQueue,
    mut is_available: impl FnMut(QueueItem) -> bool,
) -> Option<QueueItem> {
    if context.repeat() == Repeat::One {
        return None;
    }
    if let Some(next) = pending.first_matching(&mut is_available) {
        return Some(next);
    }
    context
        .peek_auto_matching(|id| is_available(QueueItem::Track(id)))
        .map(QueueItem::Track)
}

#[cfg(test)]
fn peek_auto_target(context: &Queue, pending: &UpNextQueue) -> Option<QueueItem> {
    peek_matching_auto_target(context, pending, |_| true)
}

fn previous_target(context: &mut Queue, current_pending: &mut Option<QueueItem>) -> Option<i64> {
    if current_pending.take().is_some() {
        context.current()
    } else {
        context.previous()
    }
}

fn play_pending_at(
    pending: &mut UpNextQueue,
    current_pending: &mut Option<QueueItem>,
    position: usize,
) -> Option<QueueItem> {
    let selected = pending.take_at(position)?;
    *current_pending = Some(selected);
    Some(selected)
}

impl PlayerController {
    pub(in crate::ui) fn present_queue_item(
        self: &std::rc::Rc<Self>,
        item: QueueItem,
        start: StartPlayback,
        change: crate::ui::current_track_selection::CurrentTrackChange,
    ) {
        match item {
            QueueItem::Track(id) => self.present_track(id, start, change),
            QueueItem::Episode(id) => {
                debug_assert_eq!(start, StartPlayback::Yes);
                self.player.set_next(None);
                self.play_queued_episode(id);
            }
        }
    }

    pub(in crate::ui) fn advance_playback(self: &std::rc::Rc<Self>, reason: AdvanceReason) {
        self.advance_common(reason, StartPlayback::Yes);
    }

    /// Reacts to a gapless hand-off (`PlayerEvent::AdvancedToNext`): the
    /// pre-fed next track is *already* playing, so advance the queue model by
    /// one automatic step and reflect the new track WITHOUT restarting the
    /// pipeline (`StartPlayback::No`). The model step reproduces the same id
    /// `feed_next` pre-fed, because the queue state is unchanged since the feed
    /// (every mutation re-feeds).
    pub(in crate::ui) fn advance_gaplessly(self: &std::rc::Rc<Self>) {
        self.advance_common(AdvanceReason::Automatic, StartPlayback::No);
    }

    /// Shared body of `advance_playback`/`advance_gaplessly`: compute the next
    /// track, then either start it (`StartPlayback::Yes`) or just reflect it
    /// (`No`, audio already rolling gaplessly).
    fn advance_common(self: &std::rc::Rc<Self>, reason: AdvanceReason, start: StartPlayback) {
        let live_ids = {
            let conn = &self.conn;
            match queries::query_live_track_ids(conn) {
                Ok(ids) => Some(ids),
                Err(error) => {
                    tracing::error!(%error, "failed to resolve playable queue ids; advancing without filtering");
                    None
                }
            }
        };
        let live_episode_ids = match queries::query_available_episode_ids(&self.conn) {
            Ok(ids) => Some(ids),
            Err(error) => {
                tracing::error!(%error, "failed to resolve available queued episodes; advancing without filtering them");
                None
            }
        };
        let before = self.up_next.borrow().len();
        let mut current_pending = self.current_up_next.get();
        let next = {
            let mut context = self.queue.borrow_mut();
            let mut pending = self.up_next.borrow_mut();
            if let Some(available) = live_episode_ids.as_ref() {
                drop_unavailable_episodes(&mut pending, available);
            }
            next_matching_target(
                &mut context,
                &mut pending,
                &mut current_pending,
                reason,
                |item| match item {
                    QueueItem::Track(id) => live_ids.as_ref().is_none_or(|ids| ids.contains(&id)),
                    QueueItem::Episode(id) => live_episode_ids
                        .as_ref()
                        .is_none_or(|ids| ids.contains(&id)),
                },
            )
        };
        self.current_up_next.set(current_pending);
        if self.up_next.borrow().len() != before {
            self.notify_queue_changed();
        }
        match next {
            Some(item) => self.present_queue_item(
                item,
                start,
                match reason {
                    AdvanceReason::Automatic => {
                        crate::ui::current_track_selection::CurrentTrackChange::AutomaticAdvance
                    }
                    AdvanceReason::Manual => {
                        crate::ui::current_track_selection::CurrentTrackChange::ExplicitTransport
                    }
                },
            ),
            None => {
                // PLAY-11: the filtered snapshot remains immutable while it
                // plays. Once it is exhausted, however, an already-cleared
                // Library filter may hand off to a fresh random full-library
                // snapshot instead of falling silent.
                if reason == AdvanceReason::Automatic
                    && self.refill_random_library_after_filter_clear()
                {
                    return;
                }
                // A *manual* "next" that ran off the end of an exhausted
                // queue (`Repeat::Off` — often a stale queue restored from
                // the last session) refills the queue from the currently
                // visible view because the user explicitly asked for more.
                if reason == AdvanceReason::Manual && self.refill_queue_from_view() {
                    return;
                }
                self.flush_episode_skip_toast();
                self.consecutive_skips.set(0);
                self.failure_skip_limit.set(0);
                self.reset_to_stopped();
            }
        }
    }

    fn refill_random_library_after_filter_clear(self: &std::rc::Rc<Self>) -> bool {
        let origin = self.play_origin.borrow().clone();
        let provider = self.view_refill_ids.borrow().clone();
        let Some(provider) = provider else {
            return false;
        };
        let visible_ids = provider();
        let random_live_ids = match queries::query_random_live_track_ids(&self.conn) {
            Ok(ids) => ids,
            Err(error) => {
                tracing::error!(%error, "failed to build random library continuation");
                return false;
            }
        };
        let finished_track_id = self.current_track.get().map(|(id, _)| id);
        let Some(ids) = random_library_continuation(
            origin.as_ref(),
            &visible_ids,
            random_live_ids,
            finished_track_id,
        ) else {
            return false;
        };
        let continuation_len = ids.len();
        self.play_from_view(ids, 0, super::play_origin::PlayOrigin::library());
        tracing::info!(
            continuation_len,
            "filtered queue exhausted after filter clear; continuing from random library snapshot"
        );
        true
    }

    /// Rebuilds the exhausted playback context from the visible view's ids
    /// (see `PlayerController::set_view_refill_provider`) and starts playing
    /// it — at a random position when shuffle is on, from the top otherwise.
    /// Returns whether a refill actually started playback; `false` (no
    /// provider, empty view, or the Queue view itself) leaves the caller's
    /// ordinary stop path in charge.
    fn refill_queue_from_view(self: &std::rc::Rc<Self>) -> bool {
        let provider = self.view_refill_ids.borrow().clone();
        let Some(provider) = provider else {
            return false;
        };
        let ids = provider();
        if ids.is_empty() {
            return false;
        }
        let start_index = if self.queue.borrow().is_shuffled() && ids.len() > 1 {
            let len = i32::try_from(ids.len()).unwrap_or(i32::MAX);
            usize::try_from(gtk4::glib::random_int_range(0, len)).unwrap_or(0)
        } else {
            0
        };
        let refill_len = ids.len();
        self.queue.borrow_mut().set_tracks(ids, start_index);
        self.notify_queue_changed();
        let current = self.queue.borrow().current();
        match current {
            Some(id) => {
                tracing::info!(
                    refill_len,
                    start_index,
                    "queue exhausted on manual next; refilled from the visible view"
                );
                self.play_track_id_with_change(
                    id,
                    crate::ui::current_track_selection::CurrentTrackChange::ExplicitTransport,
                );
                true
            }
            None => false,
        }
    }

    /// Pre-feeds the backend with the track that should play next, so it can
    /// hand off gaplessly on `about-to-finish`. Reads the current transition
    /// setting: when it is `Off`, clears the queued track (`set_next(None)`),
    /// so playback falls back to the ordinary `TrackFinished`-driven advance.
    /// Called after every track start and every queue/up-next/repeat/shuffle
    /// mutation, and whenever the transition setting changes — the backend
    /// keeps only the latest value.
    /// Pushes the current transition setting (mode + crossfade seconds) to the
    /// backend and re-feeds the next track, so a preference change takes effect
    /// immediately without a restart. Called at startup and from the Transitions
    /// preference handler.
    pub(in crate::ui) fn apply_transition(&self) {
        let (mode, seconds) = {
            let conn = &self.conn;
            (
                settings::get_track_transition(conn),
                settings::get_crossfade_seconds(conn),
            )
        };
        self.player.set_transition(mode, seconds);
        self.feed_next();
    }

    pub(in crate::ui) fn feed_next(&self) {
        let transition = settings::get_track_transition(&self.conn);
        if transition == TrackTransition::Off {
            self.player.set_next(None);
            return;
        }
        let live_ids = {
            let conn = &self.conn;
            match queries::query_live_track_ids(conn) {
                Ok(ids) => Some(ids),
                Err(error) => {
                    tracing::error!(%error, "failed to resolve playable pre-feed ids; pre-feeding without filtering");
                    None
                }
            }
        };
        let live_episode_ids = queries::query_available_episode_ids(&self.conn).ok();
        let next_item = {
            let context = self.queue.borrow();
            let pending = self.up_next.borrow();
            peek_matching_auto_target(&context, &pending, |item| match item {
                QueueItem::Track(id) => live_ids.as_ref().is_none_or(|ids| ids.contains(&id)),
                QueueItem::Episode(id) => live_episode_ids
                    .as_ref()
                    .is_none_or(|ids| ids.contains(&id)),
            })
        };
        let path = next_item.and_then(prefeed_track_id).and_then(|id| {
            let conn = &self.conn;
            queries::query_track_summary(conn, id)
                .ok()
                .flatten()
                .map(|summary| summary.path)
        });
        self.player.set_next(path.as_deref());
    }

    pub(in crate::ui) fn play_up_next_at(self: &std::rc::Rc<Self>, position: usize) {
        let mut current_pending = self.current_up_next.get();
        let selected = {
            let mut pending = self.up_next.borrow_mut();
            play_pending_at(&mut pending, &mut current_pending, position)
        };
        let Some(item) = selected else {
            tracing::warn!(position, "up next activation position is out of range");
            return;
        };
        self.current_up_next.set(current_pending);
        self.notify_queue_changed();
        self.present_queue_item(
            item,
            StartPlayback::Yes,
            crate::ui::current_track_selection::CurrentTrackChange::ExplicitTransport,
        );
    }

    pub(in crate::ui) fn previous_with_up_next(self: &std::rc::Rc<Self>) {
        let mut current_pending = self.current_up_next.get();
        let previous = {
            let mut context = self.queue.borrow_mut();
            previous_target(&mut context, &mut current_pending)
        };
        self.current_up_next.set(current_pending);
        match previous {
            Some(id) => self.play_track_id_with_change(
                id,
                crate::ui::current_track_selection::CurrentTrackChange::ExplicitTransport,
            ),
            None => self.reset_to_stopped(),
        }
    }
}

fn prefeed_track_id(item: QueueItem) -> Option<i64> {
    item.track_id()
}

#[cfg(test)]
mod tests {
    use reprise_core::browser::BrowserPlace;
    use reprise_core::queue::{Queue, Repeat};
    use reprise_core::up_next::UpNextQueue;
    use reprise_core::view_source::ViewSource;

    use super::{
        next_matching_target, next_target, peek_auto_target, play_pending_at, previous_target,
        random_library_continuation, AdvanceReason,
    };

    fn context(ids: &[i64]) -> Queue {
        let mut queue = Queue::new();
        queue.set_tracks(ids.to_vec(), 0);
        queue
    }

    fn pending(ids: &[i64]) -> UpNextQueue {
        let mut queue = UpNextQueue::default();
        let items = ids
            .iter()
            .copied()
            .map(reprise_core::up_next::QueueItem::Track)
            .collect::<Vec<_>>();
        queue.append(&items);
        queue
    }

    fn track(id: i64) -> Option<reprise_core::up_next::QueueItem> {
        Some(reprise_core::up_next::QueueItem::Track(id))
    }

    #[test]
    fn play_11_filter_clear_continues_with_random_full_library_snapshot() {
        let mut place = BrowserPlace::from(ViewSource::Library);
        place.track_state_mut().unwrap().search = "needle".into();
        let origin = super::super::play_origin::PlayOrigin {
            place,
            label: "Music".into(),
        };

        assert_eq!(
            random_library_continuation(
                Some(&origin),
                &[1, 2, 3],
                vec![2, 3, 1],
                Some(2),
            ),
            Some(vec![3, 1, 2]),
            "the finished filtered hit must not immediately repeat when the full library takes over"
        );
    }

    #[test]
    fn play_11_continuation_requires_a_cleared_library_filter() {
        let unfiltered = super::super::play_origin::PlayOrigin {
            place: BrowserPlace::from(ViewSource::Library),
            label: "Music".into(),
        };
        assert_eq!(
            random_library_continuation(Some(&unfiltered), &[1, 2, 3], vec![3, 1, 2], Some(2)),
            None,
            "an ordinary full-library snapshot still ends normally"
        );

        let mut filtered_place = BrowserPlace::from(ViewSource::Library);
        filtered_place.track_state_mut().unwrap().browse.genre = Some("Metal".into());
        let filtered = super::super::play_origin::PlayOrigin {
            place: filtered_place,
            label: "Music".into(),
        };
        assert_eq!(
            random_library_continuation(Some(&filtered), &[2], vec![3, 1, 2], Some(2)),
            None,
            "a filter that remains active must not loop or escape its hit set"
        );

        let mut playlist_place = BrowserPlace::from(ViewSource::Playlist(7));
        playlist_place.track_state_mut().unwrap().search = "needle".into();
        let playlist = super::super::play_origin::PlayOrigin {
            place: playlist_place,
            label: "Mix".into(),
        };
        assert_eq!(
            random_library_continuation(Some(&playlist), &[1, 2, 3], vec![3, 1, 2], Some(2)),
            None,
            "clearing a playlist filter must not escape into the whole library"
        );

        let mut one_track_place = BrowserPlace::from(ViewSource::Library);
        one_track_place.track_state_mut().unwrap().search = "only".into();
        let one_track = super::super::play_origin::PlayOrigin {
            place: one_track_place,
            label: "Music".into(),
        };
        assert_eq!(
            random_library_continuation(Some(&one_track), &[2], vec![2], Some(2)),
            None,
            "the only library title must not repeat forever"
        );
    }

    #[test]
    fn peek_auto_target_mirrors_automatic_next_without_mutating() {
        // Up-next front wins the peek, exactly like next_target(Automatic).
        let queue = context(&[1, 2, 3]);
        let up_next = pending(&[10, 20]);
        assert_eq!(peek_auto_target(&queue, &up_next), track(10));
        assert_eq!(up_next.ids(), &[10, 20]); // not consumed
        assert_eq!(queue.current(), Some(1)); // not advanced

        // No up-next: falls through to the queue's auto-advance preview.
        assert_eq!(peek_auto_target(&queue, &pending(&[])), track(2));
    }

    #[test]
    fn peek_auto_target_suppresses_gapless_on_repeat_one() {
        let mut context = context(&[1, 2, 3]);
        context.set_repeat(Repeat::One);
        // Repeat-One is intentionally not pre-fed (returns None), even with
        // pending up-next, so the loop runs through the ordinary EOS path.
        assert_eq!(peek_auto_target(&context, &pending(&[10])), None);
    }

    #[test]
    fn peek_auto_target_at_queue_end_without_repeat_is_none() {
        let mut context = context(&[1, 2, 3]);
        // Advance to the last track.
        context.next_manual();
        context.next_manual();
        assert_eq!(context.current(), Some(3));
        assert_eq!(peek_auto_target(&context, &pending(&[])), None);
    }

    #[test]
    fn queued_episode_is_never_gaplessly_prefed_as_a_track_path() {
        assert_eq!(
            super::prefeed_track_id(reprise_core::up_next::QueueItem::Episode(7)),
            None
        );
        assert_eq!(
            super::prefeed_track_id(reprise_core::up_next::QueueItem::Track(3)),
            Some(3)
        );
    }

    #[test]
    fn play_5c_advance_drops_unsubscribed_episode_without_dropping_tracks() {
        let mut pending = UpNextQueue::default();
        pending.append(&[
            reprise_core::up_next::QueueItem::Track(8),
            reprise_core::up_next::QueueItem::Episode(8),
            reprise_core::up_next::QueueItem::Episode(7),
        ]);

        assert_eq!(
            super::drop_unavailable_episodes(&mut pending, &std::collections::HashSet::from([7])),
            1
        );
        assert_eq!(
            pending.ids(),
            &[
                reprise_core::up_next::QueueItem::Track(8),
                reprise_core::up_next::QueueItem::Episode(7),
            ]
        );
    }

    #[test]
    fn pending_tracks_interrupt_then_resume_the_context() {
        let mut context = context(&[1, 2, 3]);
        let mut pending = pending(&[10, 20]);
        let mut current_pending = None;
        assert_eq!(
            next_target(
                &mut context,
                &mut pending,
                &mut current_pending,
                AdvanceReason::Automatic,
            ),
            track(10)
        );
        assert_eq!(pending.ids(), &[20]);
        assert_eq!(current_pending, track(10));
        assert_eq!(context.current(), Some(1));
        assert_eq!(
            next_target(
                &mut context,
                &mut pending,
                &mut current_pending,
                AdvanceReason::Automatic,
            ),
            track(20)
        );
        assert_eq!(
            next_target(
                &mut context,
                &mut pending,
                &mut current_pending,
                AdvanceReason::Automatic,
            ),
            track(2)
        );
        assert_eq!(current_pending, None);
    }

    #[test]
    fn availability_filter_skips_pending_and_context_candidates_without_a_fault_path() {
        let mut context = context(&[1, 2, 3]);
        let mut pending = pending(&[10, 20]);
        let mut current_pending = None;
        let available = |item: reprise_core::up_next::QueueItem| !matches!(item.id(), 10 | 2);

        assert_eq!(
            next_matching_target(
                &mut context,
                &mut pending,
                &mut current_pending,
                AdvanceReason::Automatic,
                available,
            ),
            track(20)
        );
        assert_eq!(pending.ids(), &[10]);
        assert_eq!(
            next_matching_target(
                &mut context,
                &mut pending,
                &mut current_pending,
                AdvanceReason::Automatic,
                available,
            ),
            track(3)
        );
        assert_eq!(context.ids_in_order(), vec![1, 2, 3]);
    }

    #[test]
    fn repeat_one_repeats_actual_track_only_for_automatic_advance() {
        let mut context = context(&[1, 2]);
        context.set_repeat(Repeat::One);
        let mut pending = pending(&[10, 20]);
        let mut current_pending = track(9);
        assert_eq!(
            next_target(
                &mut context,
                &mut pending,
                &mut current_pending,
                AdvanceReason::Automatic,
            ),
            track(9)
        );
        assert_eq!(pending.ids(), &[10, 20]);
        assert_eq!(
            next_target(
                &mut context,
                &mut pending,
                &mut current_pending,
                AdvanceReason::Manual,
            ),
            track(10)
        );
    }

    #[test]
    fn browse_11_repeat_one_cannot_repeat_a_loaded_catalog_tombstone() {
        let mut context = context(&[1, 2]);
        context.set_repeat(Repeat::One);
        let mut pending = pending(&[]);
        let mut current_pending = None;

        assert_eq!(
            next_matching_target(
                &mut context,
                &mut pending,
                &mut current_pending,
                AdvanceReason::Automatic,
                |id| id != 1,
            ),
            track(2)
        );
    }

    #[test]
    fn previous_from_a_pending_track_returns_to_unchanged_context() {
        let mut context = context(&[1, 2]);
        let mut current_pending = track(10);
        assert_eq!(previous_target(&mut context, &mut current_pending), Some(1));
        assert_eq!(current_pending, None);
        assert_eq!(context.current(), Some(1));
    }

    #[test]
    fn pending_only_playback_and_direct_activation_preserve_earlier_entries() {
        let mut context = Queue::new();
        let mut pending = pending(&[10, 20, 30]);
        let mut current_pending = None;
        assert_eq!(
            next_target(
                &mut context,
                &mut pending,
                &mut current_pending,
                AdvanceReason::Manual,
            ),
            track(10)
        );
        assert_eq!(
            play_pending_at(&mut pending, &mut current_pending, 1),
            track(30)
        );
        assert_eq!(pending.ids(), &[20]);
        assert_eq!(current_pending, track(30));
    }

    /// QUE-2 pin: what the composite Queue view displays (play-next FIFO,
    /// then the snapshot's play-order tail) is exactly what `next_target`
    /// plays, in exactly that order — no hidden priority.
    #[test]
    fn advance_order_equals_composite_display_order() {
        let mut context = Queue::new();
        context.set_tracks(vec![100, 200, 300], 0);
        let mut pending = pending(&[10, 20]);
        let mut current_pending = None;

        // Display order while 100 plays: play_next [10, 20], then [200, 300].
        let displayed: Vec<i64> = pending
            .ids()
            .iter()
            .map(|item| item.id())
            .chain(context.remaining_after_current())
            .collect();

        let mut played = Vec::new();
        while let Some(id) = next_target(
            &mut context,
            &mut pending,
            &mut current_pending,
            AdvanceReason::Automatic,
        ) {
            played.push(id);
        }
        assert_eq!(played, displayed);
        assert_eq!(played, vec![10, 20, 200, 300]);
    }
}
