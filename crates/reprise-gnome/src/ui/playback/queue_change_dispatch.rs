//! Queue-change fan-out, including the measured slow Now Playing refresh.

use std::cell::Cell;
use std::rc::Rc;

use super::player_controller::PlayerController;

const NOW_PLAYING_QUEUE_LISTENER_INDEX: usize = 2;

pub(super) fn clear_removed_prefed_next(
    prefed_next: &Cell<Option<i64>>,
    removed_ids: &[i64],
    clear_backend: impl FnOnce(),
) -> bool {
    if !prefed_next
        .get()
        .is_some_and(|id| removed_ids.contains(&id))
    {
        return false;
    }
    prefed_next.set(None);
    clear_backend();
    true
}

impl PlayerController {
    pub(in crate::ui) fn add_on_queue_changed(&self, callback: impl Fn() + 'static) {
        let mut callbacks = self.queue_changed.borrow_mut();
        let callback = Rc::new(callback) as Rc<dyn Fn()>;
        if callbacks.len() == NOW_PLAYING_QUEUE_LISTENER_INDEX {
            callbacks.push(super::instrumentation::defer_queue_refresh(callback));
        } else {
            callbacks.push(callback);
        }
    }

    pub(super) fn clear_prefed_next_if_removed(&self, ids: &[i64]) {
        clear_removed_prefed_next(&self.prefed_next_track, ids, || {
            self.player.set_next(None);
        });
    }

    pub(in crate::ui) fn notify_queue_changed(&self) {
        let up_next_len = self.up_next.borrow().len();
        let ((), mirror_ms) = super::instrumentation::timed(|| self.update_agent_queue_mirror());
        // Fixed order: queue model, sidebar/Queue refresh, deferred Now Playing panel.
        let callbacks = self.queue_changed.borrow().clone();
        let listener_times = super::instrumentation::time_queue_listeners(callbacks);
        // Measurements kept the gapless pre-feed synchronous. Every caller
        // holds no live queue borrow across this short operation.
        let ((), feed_ms) = super::instrumentation::timed(|| self.feed_next());
        tracing::info!(
            up_next_len,
            mirror_ms,
            listeners_ms = listener_times.total_ms,
            queue_model_ms = listener_times.queue_model_ms,
            sidebar_queue_reload_ms = listener_times.sidebar_queue_reload_ms,
            now_playing_ms = listener_times.now_playing_ms,
            feed_ms,
            "up next changed"
        );
    }
}
