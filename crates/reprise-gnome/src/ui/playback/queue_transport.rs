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

use crate::ui::current_track_selection::CurrentTrackChange;
use crate::ui::player_controller::PlayerController;
use crate::ui::up_next_transport::AdvanceReason;
use reprise_core::db::Db;
use reprise_core::media_integration::MprisPlaybackStatus;
#[cfg(test)]
use reprise_core::queue::Queue;
use reprise_core::up_next::QueueItem;
#[cfg(test)]
use reprise_core::up_next::UpNextQueue;

use super::queue_insertion::track_items;

#[path = "queue_transport_projection.rs"]
mod projection;

pub(super) fn has_direct_episode_projection(
    external: &super::external_media::ExternalPlaybackState,
) -> bool {
    projection::has_direct_episode_projection(external)
}

#[cfg(test)]
use super::queue_edit::{
    apply_queue_reorder, move_rows_to_front, remove_direct_episode_now_playing,
};

#[path = "queue_context_window.rs"]
mod queue_context_window;
pub(in crate::ui) use queue_context_window::QueueContextWindow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToggleAction {
    /// Carries the reveal the loaded track earns when it starts — the one
    /// decision that separates a cold start from every later Play (START-4).
    StartCurrent(CurrentTrackChange),
    StartPending,
    StartRandom,
    TogglePipeline,
}

#[derive(Debug, PartialEq, Eq)]
struct QueuePurgePlan {
    immediate: Vec<i64>,
    after_loaded_track: Option<i64>,
}

/// Separates a loaded catalog tombstone from every future deletion. The
/// loaded id stays as the queue playhead until playback leaves it, which
/// keeps ordinary next/previous/gapless calculations exact. Other ids are
/// purged immediately; duplicate slots of the loaded id are handled by
/// `Queue::remove_ids_except_current`.
fn queue_purge_plan(ids: &[i64], loaded: Option<i64>) -> QueuePurgePlan {
    let after_loaded_track = loaded.filter(|id| ids.contains(id));
    let mut immediate = Vec::new();
    for id in ids.iter().copied() {
        if Some(id) != after_loaded_track && !immediate.contains(&id) {
            immediate.push(id);
        }
    }
    QueuePurgePlan {
        immediate,
        after_loaded_track,
    }
}
fn should_advance_after_user_delete(ids: &[i64], loaded: Option<i64>) -> bool {
    loaded.is_some_and(|id| ids.contains(&id))
}
/// `restored_placement_intact` says the loaded track is still exactly where
/// startup routing put it: selected and centered, never played (START-4).
/// START-4 places a greeting the same way, but greeting Play bypasses this
/// decision and reaches `play_track_id` as `PlaybackStarted`, whose NAV-10b
/// reveal policy is already `MarkerOnly`. Other starts from Stopped without
/// this one-shot keep NAV-10b's explicit-transport reveal.
fn toggle_action(
    status: MprisPlaybackStatus,
    current_track: Option<QueueItem>,
    has_pending: bool,
    restored_placement_intact: bool,
) -> ToggleAction {
    match (status, current_track, has_pending) {
        (MprisPlaybackStatus::Stopped, Some(_), _) => {
            ToggleAction::StartCurrent(restored_start_change(restored_placement_intact))
        }
        (MprisPlaybackStatus::Stopped, None, true) => ToggleAction::StartPending,
        (MprisPlaybackStatus::Stopped, None, false) => ToggleAction::StartRandom,
        (MprisPlaybackStatus::Playing | MprisPlaybackStatus::Paused, _, _) => {
            ToggleAction::TogglePipeline
        }
    }
}
pub(super) fn restored_start_change(restored_placement_intact: bool) -> CurrentTrackChange {
    if restored_placement_intact {
        CurrentTrackChange::PlaybackStarted
    } else {
        CurrentTrackChange::ExplicitTransport
    }
}
pub(super) fn initial_library_availability(db: &Db) -> bool {
    reprise_core::queries::query_has_live_tracks(db)
        .inspect_err(
            |error| tracing::warn!(%error, "could not determine idle playback availability"),
        )
        .unwrap_or(false)
}
impl PlayerController {
    /// Returns every live playback-model id rejected by the core retention
    /// predicate after a scan. The caller feeds these ids into the same
    /// purge path as hard deletes and auto-clean.
    pub(in crate::ui) fn scan_queue_purge_ids(&self) -> Vec<i64> {
        let mut candidates = self.queue.borrow().ids_in_order();
        candidates.extend(
            self.up_next
                .borrow()
                .ids()
                .iter()
                .filter_map(|item| item.track_id()),
        );
        if let Some(id) = self.current_up_next.get().and_then(QueueItem::track_id) {
            candidates.push(id);
        }
        let result = {
            let conn = &self.conn;
            reprise_core::queries::query_queue_purge_track_ids(conn, &candidates)
        };
        match result {
            Ok(ids) => ids,
            Err(error) => {
                tracing::warn!(%error, "could not reconcile scan-detected queue removals");
                Vec::new()
            }
        }
    }

    pub(in crate::ui) fn purge_unavailable_episodes(&self) -> usize {
        let available = match reprise_core::queries::query_available_episode_ids(&self.conn) {
            Ok(ids) => ids,
            Err(error) => {
                tracing::warn!(%error, "could not reconcile unsubscribed queued episodes");
                return 0;
            }
        };
        let unavailable = self
            .up_next
            .borrow()
            .ids()
            .iter()
            .copied()
            .filter(|item| item.episode_id().is_some_and(|id| !available.contains(&id)))
            .collect::<Vec<_>>();
        let removed = self.up_next.borrow_mut().remove_ids(&unavailable);
        let current_removed = self
            .current_up_next
            .get()
            .is_some_and(|item| item.episode_id().is_some_and(|id| !available.contains(&id)));
        if current_removed {
            self.current_up_next.set(None);
        }
        let changed = removed + usize::from(current_removed);
        if changed > 0 {
            self.notify_queue_changed();
        }
        changed
    }

    pub(in crate::ui) fn start_current_item(
        self: &Rc<Self>,
        item: QueueItem,
        change: CurrentTrackChange,
    ) {
        let id = item.id();
        let playable = match item {
            QueueItem::Track(id) => {
                reprise_core::queries::query_live_track_ids(&self.conn).map(|ids| ids.contains(&id))
            }
            QueueItem::Episode(id) => {
                reprise_core::queries::query_available_episode_ids(&self.conn)
                    .map(|ids| ids.contains(&id))
            }
        };
        match playable {
            Ok(true) => {
                self.present_queue_item(
                    item,
                    crate::ui::player_controller::StartPlayback::Yes,
                    change,
                );
            }
            Ok(false) => self.advance_playback(AdvanceReason::Manual),
            Err(error) => {
                tracing::error!(%error, id, "could not validate restored current track; trying it directly");
                self.present_queue_item(
                    item,
                    crate::ui::player_controller::StartPlayback::Yes,
                    change,
                );
            }
        }
    }

    /// Starts the restored queue's current track while stopped; otherwise
    /// toggles the already-loaded pipeline. Shared by the bar, Space, and
    /// MPRIS PlayPause, without ever introducing startup autoplay.
    pub(in crate::ui) fn toggle_pause(self: &Rc<Self>) {
        if self.toggle_external_pause() {
            return;
        }
        let status = self
            .mpris_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .status;
        let stopped_target = (status == MprisPlaybackStatus::Stopped)
            .then(|| self.stopped_play_target())
            .flatten();
        let stopped_target = match stopped_target {
            Some(target @ super::session_player::StoppedPlayTarget::Greeting(_)) => {
                self.start_stopped_play_target(
                    target,
                    crate::ui::current_track_selection::CurrentTrackChange::ExplicitTransport,
                );
                return;
            }
            other => other,
        };
        let current = stopped_target
            .as_ref()
            .and_then(super::session_player::StoppedPlayTarget::item)
            .or_else(|| {
                self.current_up_next
                    .get()
                    .or_else(|| self.queue.borrow().current().map(QueueItem::Track))
            });
        let has_pending = !self.up_next.borrow().is_empty();
        match toggle_action(
            status,
            current,
            has_pending,
            self.restored_placement_intact.get(),
        ) {
            ToggleAction::StartCurrent(change) => {
                if let Some(target) = stopped_target {
                    self.start_stopped_play_target(target, change);
                } else if let Some(item) = current {
                    self.start_current_item(item, change);
                }
            }
            ToggleAction::StartPending => self.advance_playback(AdvanceReason::Manual),
            ToggleAction::StartRandom => {
                let snapshot = {
                    let conn = &self.conn;
                    reprise_core::queries::query_random_live_track_ids(conn)
                };
                match snapshot {
                    Ok(ids) if ids.is_empty() => {
                        self.library_has_tracks.set(false);
                        self.sync_transport_enabled(false);
                        tracing::debug!("play/pause: library is empty; nothing to play");
                    }
                    Ok(ids) => {
                        self.library_has_tracks.set(true);
                        self.play_from_view(ids, 0, super::play_origin::PlayOrigin::library());
                    }
                    Err(error) => {
                        tracing::error!(%error, "could not build random library playback snapshot");
                    }
                }
            }
            ToggleAction::TogglePipeline => {
                if let Err(error) = self.player.toggle_pause() {
                    tracing::error!(%error, "toggle play/pause failed");
                } else if status == reprise_core::media_integration::MprisPlaybackStatus::Paused {
                    self.notify_current_track(
                        crate::ui::current_track_selection::CurrentTrackChange::ExplicitTransport,
                    );
                }
            }
        }
    }

    /// Refreshes whether the idle Play action can seed a library snapshot.
    /// Track-list reloads call this after scans and library mutations.
    pub(in crate::ui) fn refresh_library_availability(&self) {
        let available = {
            let conn = &self.conn;
            reprise_core::queries::query_has_live_tracks(conn)
        };
        let available = match available {
            Ok(available) => available,
            Err(error) => {
                tracing::warn!(%error, "could not refresh idle playback availability");
                return;
            }
        };
        self.library_has_tracks.set(available);
        self.sync_transport_enabled(self.has_playable_item());
    }

    /// PLAY-14 Previous follows playback history in every mode. Episode
    /// neighbour priority is handled by `transport_previous`; reaching this
    /// method means history is the answer.
    pub(in crate::ui) fn previous(self: &Rc<Self>) {
        self.previous_with_up_next();
    }

    /// Steps the queue to the next track and plays it (or resets to stopped
    /// if there is none) — shared by the bar's next button and MPRIS's
    /// `Next` method. Same borrow discipline as `previous`.
    pub(in crate::ui) fn next(self: &Rc<Self>) {
        self.dismiss_random_start_greeting();
        if self.forward_from_history() {
            return;
        }
        if self.playback_mode() != super::preview::PlaybackMode::Queue {
            return;
        }
        self.advance_playback(AdvanceReason::Manual);
    }

    /// Starts playback of `ids[start_index]` and loads the rest of `ids` into
    /// the queue as what auto-advance/previous/next step through. Row
    /// activation lands here — see `ui::track_list`'s `queue_ids_for_
    /// activation` for how `ids`/`start_index` are built from the currently
    /// visible sort/filter view. An empty `ids` (nothing to play) resets to
    /// stopped instead of calling `play_track_id`.
    ///
    /// Borrow discipline: `set_tracks` and `current()` each run inside their
    /// own statement, so their `queue` borrows drop before `play_track_id`/
    /// `reset_to_stopped` run — see the module's `## Queue borrow
    /// discipline` doc section.
    pub fn play_from_view(
        self: &Rc<Self>,
        ids: Vec<i64>,
        start_index: usize,
        origin: super::play_origin::PlayOrigin,
    ) {
        *self.pending_random_start.borrow_mut() = None;
        self.queue.borrow_mut().set_tracks(ids, start_index);
        self.current_up_next.set(None);
        self.deferred_queue_purge_id.set(None);

        let queue_len = self.queue.borrow().len();
        // An empty seed (nothing to play) resets to stopped below and must
        // not claim an origin for a context that does not exist.
        *self.play_origin.borrow_mut() = (queue_len > 0).then_some(origin);

        tracing::info!(queue_len, start_index, "queue set from view");

        let has_transport = !self.queue.borrow().is_empty() || !self.up_next.borrow().is_empty();
        self.sync_transport_enabled(has_transport);

        let current = self.queue.borrow().current();
        match current {
            Some(id) => self.play_track_id(id),
            None => self.reset_to_stopped(),
        }
        // The Queue view and sidebar counter render the snapshot (QUE-1/
        // QUE-5), so a reseeded context is a queue change for them.
        self.notify_queue_changed();
    }

    /// The current playback context's origin, if any — clone-out so no
    /// borrow escapes (see `## Queue borrow discipline`).
    pub(in crate::ui) fn current_play_origin(&self) -> Option<super::play_origin::PlayOrigin> {
        self.play_origin.borrow().clone()
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
    /// A loaded deleted track is intentionally different: its player-owned
    /// metadata and already-open audio continue until a natural or explicit
    /// transport transition. The context retains exactly its current slot as
    /// a tombstone so next/previous and gapless prediction keep their normal
    /// cursor semantics; the Queue view omits that unresolvable row. Every
    /// duplicate/future occurrence is still removed immediately.
    pub(in crate::ui) fn purge_queue_ids(&self, ids: &[i64]) {
        if ids.is_empty() {
            return;
        }
        self.clear_prefed_next_if_removed(ids);
        let playing = self.now_playing.borrow().as_ref().map(|track| track.id);
        let plan = queue_purge_plan(ids, playing);
        let playing_from_up_next = plan.after_loaded_track.is_some()
            && self.current_up_next.get().and_then(QueueItem::track_id) == plan.after_loaded_track;
        let context_changed = if playing_from_up_next {
            self.queue.borrow_mut().remove_ids(ids)
        } else {
            let mut queue = self.queue.borrow_mut();
            let immediate = queue.remove_ids(&plan.immediate);
            let duplicates = plan
                .after_loaded_track
                .is_some_and(|id| queue.remove_ids_except_current(&[id]) > 0);
            immediate || duplicates
        };
        let items = track_items(ids);
        let pending_changed = self.up_next.borrow_mut().remove_ids(&items) > 0;
        if self
            .current_up_next
            .get()
            .and_then(QueueItem::track_id)
            .is_some_and(|id| ids.contains(&id))
            && !playing_from_up_next
        {
            self.current_up_next.set(None);
        }
        if let Some(id) = plan.after_loaded_track {
            self.deferred_queue_purge_id.set(Some(id));
        }
        if context_changed || pending_changed {
            tracing::info!(
                removed = ids.len(),
                queue_len = self.up_next.borrow().len(),
                "queue purged of hard-deleted track ids"
            );
            // Both lists are visible now (composite Queue view + QUE-5
            // pending counter), so a context-only purge must refresh too
            // (adversarial review, queue+nav plan, finding 3).
            self.notify_queue_changed();
        } else if plan.after_loaded_track.is_some() {
            // The Queue projection changed even when there were no future
            // entries to remove: its dead Now Playing row is now omitted.
            self.notify_queue_changed();
        }
        if let Some(id) = plan.after_loaded_track {
            tracing::info!(
                deleted = id,
                "loaded track left playing from its owned snapshot after catalog deletion"
            );
        }
    }

    /// Explicit Remove/Trash is a transport action when it deleted the
    /// loaded track. Background purge callers intentionally use only
    /// `purge_queue_ids`, preserving PLAY-5a/PLAY-5b's no-interruption rule.
    pub(in crate::ui) fn advance_after_user_catalog_delete(self: &Rc<Self>, ids: &[i64]) {
        let loaded = self.now_playing.borrow().as_ref().map(|track| track.id);
        if !should_advance_after_user_delete(ids, loaded) {
            return;
        }
        tracing::info!(deleted = ?loaded, "user deleted the loaded track; advancing playback");
        self.advance_playback(AdvanceReason::Automatic);
    }
}

#[cfg(test)]
#[path = "queue_transport_tests.rs"]
mod tests;
