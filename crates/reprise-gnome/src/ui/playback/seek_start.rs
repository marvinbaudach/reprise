//! Waveform-only start-then-seek handling for restored media.

use std::rc::Rc;

use reprise_core::media_integration::MprisPlaybackStatus;
use reprise_core::up_next::QueueItem;

use super::external_media_state::ResumePolicy;
use super::player_controller::PlayerController;
use super::queue_transport::restored_start_change;

impl PlayerController {
    pub(in crate::ui) fn seek_or_start(self: &Rc<Self>, position_ms: i64) {
        if self.seek_restored_episode_at(position_ms) {
            return;
        }
        let status = self
            .mpris_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .status;
        if status == MprisPlaybackStatus::Stopped {
            let current = self
                .current_up_next
                .get()
                .or_else(|| self.queue.borrow().current().map(QueueItem::Track));
            if let Some(item) = current {
                let change = restored_start_change(self.restored_placement_intact.get());
                self.start_current_item(item, change);
                match item {
                    QueueItem::Track(id)
                        if self
                            .current_track
                            .get()
                            .is_some_and(|(loaded, _)| loaded == id) =>
                    {
                        self.start_pending_seek(position_ms);
                    }
                    QueueItem::Episode(_) => self.seek(position_ms),
                    QueueItem::Track(_) => {}
                }
                return;
            }
        }
        self.seek(position_ms);
    }

    fn start_pending_seek(&self, position_ms: i64) {
        let mut policy = ResumePolicy::new(position_ms);
        let succeeded = self.seek_after_start(position_ms);
        policy.initial_seek_finished(succeeded);
        *self.pending_local_seek.borrow_mut() =
            (!succeeded && position_ms > 0).then_some(policy);
    }

    pub(in crate::ui) fn seek_after_start(&self, position_ms: i64) -> bool {
        self.try_seek_with_feedback(position_ms)
    }

    pub(in crate::ui) fn retry_pending_local_seek(&self, duration_ms: i64) {
        let Some(mut policy) = self.pending_local_seek.borrow_mut().take() else {
            return;
        };
        if duration_ms <= 0 {
            *self.pending_local_seek.borrow_mut() = Some(policy);
            return;
        }
        if let Some(position_ms) = policy.position_tick(duration_ms) {
            self.seek(position_ms);
        }
    }

    pub(in crate::ui) fn clear_pending_local_seek(&self) {
        self.pending_local_seek.borrow_mut().take();
    }
}

#[cfg(test)]
mod tests {
    use super::ResumePolicy;

    #[test]
    fn local_start_seek_reuses_resume_policy_for_exactly_one_retry() {
        let mut policy = ResumePolicy::new(30_000);
        policy.initial_seek_finished(false);

        assert_eq!(policy.position_tick(0), None);
        assert_eq!(policy.position_tick(180_000), Some(30_000));
        assert_eq!(policy.position_tick(180_000), None);
    }
}
