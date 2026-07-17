//! Playback-context and manual Up Next transport methods, split out of
//! `player_controller.rs` to keep that file under the project's file-size
//! limit. The hidden `queue` remains the selected Library/playlist context;
//! the visible Queue source is backed only by `up_next`.
//!
//! These are `pub(in crate::ui)` so `mpris_mirror.rs`'s `handle_mpris_command` (and,
//! for `queue_ids_snapshot`, `track_list.rs`) can call them too — shared with
//! the bar's own button clicks so a physical media key and the on-screen
//! control run exactly one code path (DRY).
//!
//! Borrow discipline: every `queue` access here follows `player_controller.
//! rs`'s `## Queue borrow discipline` doc section — each borrow runs inside
//! its own statement/block, dropped before any call that could re-enter this
//! controller.

use std::rc::Rc;

use crate::ui::player_controller::{PlayerController, StartPlayback};
use crate::ui::up_next_transport::AdvanceReason;
use reprise_core::media_integration::MprisPlaybackStatus;
use reprise_core::queue::Queue;
use reprise_core::up_next::UpNextQueue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToggleAction {
    StartCurrent,
    StartPending,
    TogglePipeline,
    Noop,
}

/// The track that should take over when the *currently playing* one was
/// hard-deleted out from under the pipeline (see `purge_queue_ids`). Runs
/// AFTER the purge has already mutated both queues, which is the one thing
/// that keeps it from mirroring `up_next_transport.rs`'s `next_target`:
///
/// - Pending Up Next wins, exactly as it would on a natural end-of-track.
/// - Otherwise the context takes over, and *how* depends on where the deleted
///   track came from. Playing from the context: `Queue::remove_ids` has
///   already stepped the cursor onto the next survivor, so `current()` IS the
///   successor and advancing again would skip a track. Playing from Up Next:
///   the cursor still sits on the context track that played *before* the
///   interjection, so it must step forward like `next_target` does.
///
/// Repeat mode deliberately gets no say: `Repeat::One` cannot loop a track
/// that no longer exists, and `advance_auto` still honours `Repeat::All` for
/// the Up Next case.
fn successor_after_purge(
    context: &mut Queue,
    pending: &mut UpNextQueue,
    current_pending: &mut Option<i64>,
    was_playing_from_up_next: bool,
) -> Option<i64> {
    if let Some(next) = pending.pop_front() {
        *current_pending = Some(next);
        return Some(next);
    }
    *current_pending = None;
    if was_playing_from_up_next {
        context.advance_auto()
    } else {
        context.current()
    }
}

fn toggle_action(
    status: MprisPlaybackStatus,
    current_track: Option<i64>,
    has_pending: bool,
) -> ToggleAction {
    match (status, current_track, has_pending) {
        (MprisPlaybackStatus::Stopped, Some(_), _) => ToggleAction::StartCurrent,
        (MprisPlaybackStatus::Stopped, None, true) => ToggleAction::StartPending,
        (MprisPlaybackStatus::Stopped, None, false) => ToggleAction::Noop,
        (MprisPlaybackStatus::Playing | MprisPlaybackStatus::Paused, _, _) => {
            ToggleAction::TogglePipeline
        }
    }
}

impl PlayerController {
    pub(in crate::ui) fn set_on_queue_changed(&self, callback: impl Fn() + 'static) {
        *self.queue_changed.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn notify_queue_changed(&self) {
        tracing::info!(up_next_len = self.up_next.borrow().len(), "up next changed");
        let callback = self.queue_changed.borrow().clone();
        if let Some(callback) = callback {
            callback();
        }
        // The up-next front / queue order may have changed, so the upcoming
        // track changed: re-feed the gapless next. All up-next edits funnel
        // through here. `feed_next` only takes short, sequential borrows, and
        // every caller of `notify_queue_changed` holds no live borrow across
        // it (see `## Queue borrow discipline`).
        self.feed_next();
    }

    /// Starts the restored queue's current track while stopped; otherwise
    /// toggles the already-loaded pipeline. Shared by the bar, Space, and
    /// MPRIS PlayPause, without ever introducing startup autoplay.
    pub(in crate::ui) fn toggle_pause(&self) {
        let status = self
            .mpris_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .status;
        let current = self
            .current_up_next
            .get()
            .or_else(|| self.queue.borrow().current());
        let has_pending = !self.up_next.borrow().is_empty();
        match toggle_action(status, current, has_pending) {
            ToggleAction::StartCurrent => {
                if let Some(id) = current {
                    self.play_track_id(id);
                }
            }
            ToggleAction::StartPending => self.advance_playback(AdvanceReason::Manual),
            ToggleAction::TogglePipeline => {
                if let Err(error) = self.player.toggle_pause() {
                    tracing::error!(%error, "toggle play/pause failed");
                }
            }
            ToggleAction::Noop => tracing::debug!("play/pause: queue is empty; nothing to play"),
        }
    }

    /// Steps the queue to the previous track and plays it (or resets to
    /// stopped if there is none) — shared by the bar's previous button and
    /// MPRIS's `Previous` method. Borrow discipline: `previous()` runs
    /// inside this one `let` statement, so the borrow drops before
    /// `play_track_id`/`reset_to_stopped` run.
    pub(in crate::ui) fn previous(&self) {
        self.previous_with_up_next();
    }

    /// Steps the queue to the next track and plays it (or resets to stopped
    /// if there is none) — shared by the bar's next button and MPRIS's
    /// `Next` method. Same borrow discipline as `previous`.
    pub(in crate::ui) fn next(&self) {
        self.advance_playback(AdvanceReason::Manual);
    }

    /// Appends explicit user selections to visible Up Next without replacing
    /// or starting the hidden playback context. Duplicates remain meaningful
    /// user choices; an empty slice is a no-op.
    pub(in crate::ui) fn append_to_queue(&self, ids: &[i64]) {
        if ids.is_empty() {
            tracing::debug!("append to queue: nothing to add; ignoring");
            return;
        }
        self.up_next.borrow_mut().append(ids);
        self.notify_queue_changed();
        let queue_len = self.up_next.borrow().len();
        self.sync_transport_enabled(true);
        tracing::info!(added = ids.len(), queue_len, "tracks added to queue");
    }

    /// Snapshot of pending manual ids in stable visible order. The Queue view
    /// asks for a fresh owned value on each reload, so consumption, removal,
    /// and drag reorder cannot expose the hidden context or a stale list.
    pub(in crate::ui) fn queue_ids_snapshot(&self) -> Vec<i64> {
        self.up_next.borrow().ids().to_vec()
    }

    pub(in crate::ui) fn up_next_len(&self) -> usize {
        self.up_next.borrow().len()
    }

    pub(in crate::ui) fn remove_up_next_positions(&self, positions: &[usize]) -> usize {
        let removed = self.up_next.borrow_mut().remove_positions(positions);
        if removed > 0 {
            self.notify_queue_changed();
        }
        removed
    }

    /// Reorders pending manual entries only. The caller reloads Queue after a
    /// successful mutation; invalid and no-op positions return `false`.
    pub(in crate::ui) fn move_queue_item(&self, from: usize, to: usize) -> bool {
        self.up_next.borrow_mut().move_item(from, to)
    }

    /// Purges hard-deleted track ids from the queue (Stage-3 close-out):
    /// "Remove from library" (`queries::remove_missing_tracks`) deletes
    /// `tracks` rows outright — without this, a queued id that no longer
    /// resolves to a row desyncs `Queue::len`/`ids_in_order` from what
    /// `ViewSource::Queue`'s window query can actually render (see
    /// `queries.rs`'s module doc, `Queue` section, and `query_track_count`'s
    /// `Queue` arm). Called from `ui::track_list_context_menu::handle_
    /// remove_from_library` with exactly the ids `remove_missing_tracks`
    /// reports as actually deleted — never the raw requested selection,
    /// which could include ids that turned out not to be missing any more
    /// and so were never deleted. A no-op for an empty slice (no `queue`
    /// borrow taken at all).
    ///
    /// Purging the models is only half the job: when the deleted id is the
    /// track the pipeline is *currently playing*, this also hands playback to
    /// its successor (`successor_after_purge`), or stops when nothing
    /// survives. Nothing else would — deleting never attempts a `play()`, so
    /// `handle_unplayable_track`'s skip path can't fire, and the audio itself
    /// is unaffected by the file going away (trashing is a rename, and even a
    /// real unlink leaves an open descriptor's inode playable). Without this,
    /// a trashed track keeps playing to its end.
    pub(in crate::ui) fn purge_queue_ids(&self, ids: &[i64]) {
        if ids.is_empty() {
            return;
        }
        // Read before the purge below clears `current_up_next`: which track
        // the user is actually hearing, and whether it came from Up Next
        // rather than the playback context. `now_playing` is the loaded
        // track's identity (the same source `present_track` compares its
        // `previous_id` against); the borrow ends inside this statement.
        let playing = self.now_playing.borrow().as_ref().map(|track| track.id);
        let playing_purged = playing.is_some_and(|id| ids.contains(&id));
        let played_from_up_next = playing_purged && self.current_up_next.get() == playing;

        let context_changed = self.queue.borrow_mut().remove_ids(ids);
        let pending_changed = self.up_next.borrow_mut().remove_ids(ids);
        if self
            .current_up_next
            .get()
            .is_some_and(|id| ids.contains(&id))
        {
            self.current_up_next.set(None);
        }
        if context_changed || pending_changed {
            tracing::info!(
                removed = ids.len(),
                queue_len = self.up_next.borrow().len(),
                "queue purged of hard-deleted track ids"
            );
            if pending_changed {
                self.notify_queue_changed();
            }
        }

        if !playing_purged {
            return;
        }
        // The pipeline is still playing the deleted file — trashing is a
        // rename, and an already-open descriptor keeps its inode alive — so
        // nothing stops on its own. Take over: hand playback to the
        // successor, or stop when nothing survives.
        let before = self.up_next.borrow().len();
        let mut current_pending = self.current_up_next.get();
        let successor = {
            let mut context = self.queue.borrow_mut();
            let mut pending = self.up_next.borrow_mut();
            successor_after_purge(
                &mut context,
                &mut pending,
                &mut current_pending,
                played_from_up_next,
            )
        };
        self.current_up_next.set(current_pending);
        if self.up_next.borrow().len() != before {
            self.notify_queue_changed();
        }
        match successor {
            Some(next) => {
                tracing::info!(
                    deleted = ?playing,
                    next,
                    "the playing track was deleted; skipping to its successor"
                );
                self.present_track(next, StartPlayback::Yes);
            }
            None => {
                tracing::info!(
                    deleted = ?playing,
                    "the playing track was deleted and nothing follows it; stopping"
                );
                self.consecutive_skips.set(0);
                self.failure_skip_limit.set(0);
                self.reset_to_stopped();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::queue::Repeat;

    /// Context queue seeded with `ids`, currently playing the one at
    /// `start_index` — the ordinary "playing from the Library view" shape.
    fn context(ids: &[i64], start_index: usize) -> Queue {
        let mut queue = Queue::new();
        queue.set_tracks(ids.to_vec(), start_index);
        queue
    }

    fn pending(ids: &[i64]) -> UpNextQueue {
        let mut up_next = UpNextQueue::default();
        up_next.append(ids);
        up_next
    }

    #[test]
    fn purging_the_playing_context_track_plays_the_next_surviving_one() {
        let mut queue = context(&[10, 20, 30], 0);
        let mut up_next = pending(&[]);
        let mut current_pending = None;
        // `purge_queue_ids` runs this first; it already steps the cursor onto
        // the next survivor, which is why the successor must NOT advance again.
        queue.remove_ids(&[10]);

        let next = successor_after_purge(&mut queue, &mut up_next, &mut current_pending, false);

        assert_eq!(next, Some(20));
        assert_eq!(current_pending, None);
    }

    #[test]
    fn purging_the_playing_context_track_prefers_a_pending_up_next_track() {
        let mut queue = context(&[10, 20], 0);
        let mut up_next = pending(&[99]);
        let mut current_pending = None;
        queue.remove_ids(&[10]);

        let next = successor_after_purge(&mut queue, &mut up_next, &mut current_pending, false);

        assert_eq!(next, Some(99));
        assert_eq!(current_pending, Some(99));
        assert!(up_next.is_empty());
    }

    #[test]
    fn purging_the_playing_up_next_track_steps_the_context_forward() {
        // The context cursor still sits on 10 — the track that played before
        // the up-next interjection — so resuming it would replay it.
        let mut queue = context(&[10, 20], 0);
        let mut up_next = pending(&[]);
        let mut current_pending = None;

        let next = successor_after_purge(&mut queue, &mut up_next, &mut current_pending, true);

        assert_eq!(next, Some(20));
        assert_eq!(current_pending, None);
    }

    #[test]
    fn purging_the_last_surviving_track_stops_playback() {
        let mut queue = context(&[10], 0);
        let mut up_next = pending(&[]);
        let mut current_pending = None;
        queue.remove_ids(&[10]);

        let next = successor_after_purge(&mut queue, &mut up_next, &mut current_pending, false);

        assert_eq!(next, None);
    }

    #[test]
    fn purging_the_playing_track_under_repeat_one_moves_on_instead_of_looping() {
        // Repeat::One cannot repeat a track that no longer exists, so the
        // deleted track's successor wins over the repeat mode.
        let mut queue = context(&[10, 20], 0);
        queue.set_repeat(Repeat::One);
        let mut up_next = pending(&[]);
        let mut current_pending = None;
        queue.remove_ids(&[10]);

        let next = successor_after_purge(&mut queue, &mut up_next, &mut current_pending, false);

        assert_eq!(next, Some(20));
    }

    #[test]
    fn stopped_toggle_starts_current_queue_track_without_autoplay() {
        assert_eq!(
            toggle_action(MprisPlaybackStatus::Stopped, Some(42), false),
            ToggleAction::StartCurrent
        );
        assert_eq!(
            toggle_action(MprisPlaybackStatus::Stopped, None, true),
            ToggleAction::StartPending
        );
        assert_eq!(
            toggle_action(MprisPlaybackStatus::Stopped, None, false),
            ToggleAction::Noop
        );
    }
}
