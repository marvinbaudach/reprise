use reprise_core::library::settings::{self, TrackTransition};
use reprise_core::queries;
use reprise_core::queue::{Queue, Repeat};
use reprise_core::up_next::UpNextQueue;

use crate::ui::player_controller::{PlayerController, StartPlayback};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum AdvanceReason {
    Automatic,
    Manual,
}

fn next_target(
    context: &mut Queue,
    pending: &mut UpNextQueue,
    current_pending: &mut Option<i64>,
    reason: AdvanceReason,
) -> Option<i64> {
    if reason == AdvanceReason::Automatic && context.repeat() == Repeat::One {
        if let Some(current) = current_pending.or_else(|| context.current()) {
            return Some(current);
        }
    }

    if let Some(next) = pending.pop_front() {
        *current_pending = Some(next);
        return Some(next);
    }

    *current_pending = None;
    match reason {
        AdvanceReason::Automatic => context.advance_auto(),
        AdvanceReason::Manual => context.next_manual(),
    }
}

/// Non-mutating preview of the track an `Automatic` advance would select next,
/// for the gapless pre-feed (`feed_next`). Mirrors `next_target`'s `Automatic`
/// branch WITHOUT mutating the queue or up-next.
///
/// Repeat-One is deliberately excluded (returns `None`): looping the same track
/// keeps running through the ordinary end-of-stream path so play-tracking stays
/// a single, well-understood flow and we avoid a same-URI `about-to-finish`
/// edge case. The tiny gap on a repeat-one loop is an accepted trade-off.
fn peek_auto_target(context: &Queue, pending: &UpNextQueue) -> Option<i64> {
    if context.repeat() == Repeat::One {
        return None;
    }
    if let Some(&next) = pending.ids().first() {
        return Some(next);
    }
    context.peek_auto()
}

fn previous_target(context: &mut Queue, current_pending: &mut Option<i64>) -> Option<i64> {
    if current_pending.take().is_some() {
        context.current()
    } else {
        context.previous()
    }
}

fn play_pending_at(
    pending: &mut UpNextQueue,
    current_pending: &mut Option<i64>,
    position: usize,
) -> Option<i64> {
    let selected = pending.take_through(position)?;
    *current_pending = Some(selected);
    Some(selected)
}

impl PlayerController {
    pub(in crate::ui) fn advance_playback(&self, reason: AdvanceReason) {
        self.advance_common(reason, StartPlayback::Yes);
    }

    /// Reacts to a gapless hand-off (`PlayerEvent::AdvancedToNext`): the
    /// pre-fed next track is *already* playing, so advance the queue model by
    /// one automatic step and reflect the new track WITHOUT restarting the
    /// pipeline (`StartPlayback::No`). The model step reproduces the same id
    /// `feed_next` pre-fed, because the queue state is unchanged since the feed
    /// (every mutation re-feeds).
    pub(in crate::ui) fn advance_gaplessly(&self) {
        self.advance_common(AdvanceReason::Automatic, StartPlayback::No);
    }

    /// Shared body of `advance_playback`/`advance_gaplessly`: compute the next
    /// track, then either start it (`StartPlayback::Yes`) or just reflect it
    /// (`No`, audio already rolling gaplessly).
    fn advance_common(&self, reason: AdvanceReason, start: StartPlayback) {
        let before = self.up_next.borrow().len();
        let mut current_pending = self.current_up_next.get();
        let next = {
            let mut context = self.queue.borrow_mut();
            let mut pending = self.up_next.borrow_mut();
            next_target(&mut context, &mut pending, &mut current_pending, reason)
        };
        self.current_up_next.set(current_pending);
        if self.up_next.borrow().len() != before {
            self.notify_queue_changed();
        }
        match next {
            Some(id) => self.present_track(id, start),
            None => {
                // A *manual* "next" that ran off the end of an exhausted
                // queue (`Repeat::Off` — often a stale queue restored from
                // the last session) refills the queue from the currently
                // visible view instead of going silent; the user explicitly
                // asked for more music. A natural (automatic) end of the
                // queue still stops, as it should.
                if reason == AdvanceReason::Manual && self.refill_queue_from_view() {
                    return;
                }
                self.consecutive_skips.set(0);
                self.failure_skip_limit.set(0);
                self.reset_to_stopped();
            }
        }
    }

    /// Rebuilds the exhausted playback context from the visible view's ids
    /// (see `PlayerController::set_view_refill_provider`) and starts playing
    /// it — at a random position when shuffle is on, from the top otherwise.
    /// Returns whether a refill actually started playback; `false` (no
    /// provider, empty view, or the Queue view itself) leaves the caller's
    /// ordinary stop path in charge.
    fn refill_queue_from_view(&self) -> bool {
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
                self.play_track_id(id);
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
            let conn = self.conn.borrow();
            (
                settings::get_track_transition(&conn),
                settings::get_crossfade_seconds(&conn),
            )
        };
        self.player.set_transition(mode, seconds);
        self.feed_next();
    }

    pub(in crate::ui) fn feed_next(&self) {
        let transition = settings::get_track_transition(&self.conn.borrow());
        if transition == TrackTransition::Off {
            self.player.set_next(None);
            return;
        }
        let next_id = {
            let context = self.queue.borrow();
            let pending = self.up_next.borrow();
            peek_auto_target(&context, &pending)
        };
        let path = next_id.and_then(|id| {
            let conn = self.conn.borrow();
            queries::query_track_summary(&conn, id)
                .ok()
                .flatten()
                .map(|summary| summary.path)
        });
        self.player.set_next(path.as_deref());
    }

    pub(in crate::ui) fn play_up_next_at(&self, position: usize) {
        let mut current_pending = self.current_up_next.get();
        let selected = {
            let mut pending = self.up_next.borrow_mut();
            play_pending_at(&mut pending, &mut current_pending, position)
        };
        let Some(id) = selected else {
            tracing::warn!(position, "up next activation position is out of range");
            return;
        };
        self.current_up_next.set(current_pending);
        self.notify_queue_changed();
        self.play_track_id(id);
    }

    pub(in crate::ui) fn previous_with_up_next(&self) {
        let mut current_pending = self.current_up_next.get();
        let previous = {
            let mut context = self.queue.borrow_mut();
            previous_target(&mut context, &mut current_pending)
        };
        self.current_up_next.set(current_pending);
        match previous {
            Some(id) => self.play_track_id(id),
            None => self.reset_to_stopped(),
        }
    }
}

#[cfg(test)]
mod tests {
    use reprise_core::queue::{Queue, Repeat};
    use reprise_core::up_next::UpNextQueue;

    use super::{next_target, peek_auto_target, play_pending_at, previous_target, AdvanceReason};

    fn context(ids: &[i64]) -> Queue {
        let mut queue = Queue::new();
        queue.set_tracks(ids.to_vec(), 0);
        queue
    }

    fn pending(ids: &[i64]) -> UpNextQueue {
        let mut queue = UpNextQueue::default();
        queue.append(ids);
        queue
    }

    #[test]
    fn peek_auto_target_mirrors_automatic_next_without_mutating() {
        // Up-next front wins the peek, exactly like next_target(Automatic).
        let queue = context(&[1, 2, 3]);
        let up_next = pending(&[10, 20]);
        assert_eq!(peek_auto_target(&queue, &up_next), Some(10));
        assert_eq!(up_next.ids(), &[10, 20]); // not consumed
        assert_eq!(queue.current(), Some(1)); // not advanced

        // No up-next: falls through to the queue's auto-advance preview.
        assert_eq!(peek_auto_target(&queue, &pending(&[])), Some(2));
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
            Some(10)
        );
        assert_eq!(pending.ids(), &[20]);
        assert_eq!(current_pending, Some(10));
        assert_eq!(context.current(), Some(1));
        assert_eq!(
            next_target(
                &mut context,
                &mut pending,
                &mut current_pending,
                AdvanceReason::Automatic,
            ),
            Some(20)
        );
        assert_eq!(
            next_target(
                &mut context,
                &mut pending,
                &mut current_pending,
                AdvanceReason::Automatic,
            ),
            Some(2)
        );
        assert_eq!(current_pending, None);
    }

    #[test]
    fn repeat_one_repeats_actual_track_only_for_automatic_advance() {
        let mut context = context(&[1, 2]);
        context.set_repeat(Repeat::One);
        let mut pending = pending(&[10, 20]);
        let mut current_pending = Some(9);
        assert_eq!(
            next_target(
                &mut context,
                &mut pending,
                &mut current_pending,
                AdvanceReason::Automatic,
            ),
            Some(9)
        );
        assert_eq!(pending.ids(), &[10, 20]);
        assert_eq!(
            next_target(
                &mut context,
                &mut pending,
                &mut current_pending,
                AdvanceReason::Manual,
            ),
            Some(10)
        );
    }

    #[test]
    fn previous_from_a_pending_track_returns_to_unchanged_context() {
        let mut context = context(&[1, 2]);
        let mut current_pending = Some(10);
        assert_eq!(previous_target(&mut context, &mut current_pending), Some(1));
        assert_eq!(current_pending, None);
        assert_eq!(context.current(), Some(1));
    }

    #[test]
    fn pending_only_playback_and_direct_activation_are_supported() {
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
            Some(10)
        );
        assert_eq!(
            play_pending_at(&mut pending, &mut current_pending, 1),
            Some(30)
        );
        assert!(pending.is_empty());
        assert_eq!(current_pending, Some(30));
    }
}
