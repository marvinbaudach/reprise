use reprise_core::queue::{Queue, Repeat};
use reprise_core::up_next::UpNextQueue;

use crate::ui::player_controller::PlayerController;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AdvanceReason {
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
    pub(super) fn advance_playback(&self, reason: AdvanceReason) {
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
            Some(id) => self.play_track_id(id),
            None => {
                self.consecutive_skips.set(0);
                self.failure_skip_limit.set(0);
                self.reset_to_stopped();
            }
        }
    }

    pub(super) fn play_up_next_at(&self, position: usize) {
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

    pub(super) fn previous_with_up_next(&self) {
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

    use super::{next_target, play_pending_at, previous_target, AdvanceReason};

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
