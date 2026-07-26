//! MPRIS mirror updates and command handling for `PlayerController` (Stage 2
//! Task 6; split out of `player_controller.rs` in Stage 3 Task 1; extended to
//! the full Player surface — Position/Seek, Shuffle, LoopStatus, Rate,
//! Volume — in Stage 3 Task 10. See `player_controller.rs`'s `## MPRIS` doc
//! section for the parts of the MPRIS story that stayed there: field
//! ownership (`mpris_state`/`now_playing`/`volume`/`mpris_seek_notify`),
//! starting the D-Bus thread in `PlayerController::new`, the drain loop that
//! calls `handle_mpris_command` below once per received command, and the
//! `seek` method (also called directly by the bar's seek scale) that both
//! actually seeks and triggers the `Seeked` signal via `notify_mpris_seek`.
//!
//! ## What lives here
//!
//! - `update_mpris_mirror`: recomputes `mpris::MprisState` from the
//!   controller's current playback state and writes it into the shared
//!   `Arc<Mutex<mpris::MprisState>>` the D-Bus thread reads. Covers every
//!   controller-owned field except `position_ms` (see its own doc comment)
//!   — status, track metadata, transport-enabled flags, and (Stage 3 Task
//!   10) `shuffle`/`repeat` (read fresh from `Queue` every call) and `volume`
//!   (read from `PlayerController::volume`, `Player` having no getter of its
//!   own). `art_url` starts empty and is retained in `now_playing` once the
//!   off-thread cover loader patches both caches.
//! - `update_mpris_position`/`update_mpris_volume`/`update_mpris_shuffle`/
//!   `update_mpris_repeat`: narrow, single-field patches to the mirror for
//!   state that can change *without* a status/track transition — a position
//!   tick, a volume/shuffle/repeat change from either the bar or MPRIS
//!   itself. See each function's own doc comment for why a targeted patch
//!   (not a full `update_mpris_mirror` rebuild) is the right shape for each.
//! - `notify_mpris_seek`: tells `mpris.rs`'s relay thread to emit `Seeked`.
//! - `handle_mpris_command`/`mpris_play`/`mpris_pause`/`mpris_status`/
//!   `mpris_seek_relative`/`mpris_set_shuffle`/`mpris_set_loop`/`mpris_set_
//!   volume`: applies one `MprisCommand` received from that thread — the
//!   MPRIS drain loop's only caller.
//! - `mpris_status_from_playback_state`: the one explicit conversion between
//!   `player::PlaybackState` and `mpris::MprisPlaybackStatus` (deliberately
//!   separate types — see `mpris.rs`'s own doc comment for why).
//!
//! ## Seam: `pub(super)`, not `pub(crate)`
//!
//! This module is a *sibling* of `player_controller` under `ui` (both
//! declared in `ui/mod.rs`), not a descendant of it — Rust's privacy rules
//! only extend a private item's visibility to the item's defining module and
//! that module's descendants, so reaching into `PlayerController`'s
//! internals from here needs explicit, narrow grants. `player_controller.rs`
//! marks exactly the fields (`queue`, `mpris_state`, `now_playing`, `volume`,
//! `mpris_seek_notify`) and methods (`play_track_id`, `toggle_pause`, `next`,
//! `previous`, `reset_to_stopped`, `seek`) this module touches as
//! `pub(super)` — visible throughout `ui` and its descendants, but no wider
//! (deliberately not `pub(crate)`, let alone `pub`: nothing outside `ui`
//! needs any of this). `player_controller.rs` still owns every field
//! outright; this module (like `playback_faults.rs`) only ever borrows
//! `&self` via `impl PlayerController` blocks defined here — inherent impls
//! for one type are allowed to span multiple modules/files within a crate,
//! so no new type or wrapper is needed to split the behavior out.
//!
//! ## Queue borrow discipline
//!
//! Same invariant as `player_controller.rs`'s `## Queue borrow discipline`
//! doc section — read it there for the full rationale (three prior review
//! catches motivate it): no `queue` `Ref`/`RefMut` may still be alive when a
//! call that can re-enter the player/GTK runs. `mpris_play` reads `queue.
//! borrow().current()` inside its own `let` statement, dropping the borrow
//! before the `self.play_track_id(id)` call below it. `update_mpris_mirror`
//! reads `queue.borrow().is_empty()`/`is_shuffled()`/`repeat()` each inside
//! their own statement — none of them call back out afterward, so this isn't
//! actually a re-entrancy hazard here — but the hoist-into-one-statement
//! shape is kept consistent anyway. `mpris_set_shuffle`/`mpris_set_loop`
//! mutate the queue and then call a `bar`/mirror update afterward; the same
//! one-statement-per-queue-call shape applies there too, even though neither
//! `PlayerBar`'s setters nor the mirror patch functions can re-enter `queue`.

use std::rc::Rc;

use gtk4::glib;

use crate::ui::player_controller::PlayerController;
use reprise_core::media_integration::{self, MprisCommand, MprisPlaybackStatus, MprisState};
use reprise_core::playback::PlaybackState;
use reprise_core::queue::Repeat;

const AGENT_QUEUE_WINDOW: usize = 200;

/// Spawns the MPRIS-command drain loop: `controller`'s `new` (Stage-3
/// close-out: moved here from that function to keep `player_controller.rs`
/// under the split-file line gate) calls this once, right after starting the
/// `PlayerEvent` drain loop it keeps for itself. Mirrors that loop's shape
/// exactly (see `player_controller.rs`'s `## MPRIS` doc section): a `Weak`
/// controller reference — never a strong `Rc`, which would leak the
/// controller for the app's whole lifetime — upgraded once per iteration,
/// breaking the loop the first time the upgrade fails (i.e. once the
/// controller itself has dropped), draining `receiver` (the `async_channel::
/// Receiver<MprisCommand>` half of the channel `mpris::start` hands back) in
/// strict FIFO order, and applying exactly one command per iteration via
/// `handle_mpris_command`.
pub(super) fn spawn_command_drain(
    controller: &Rc<PlayerController>,
    receiver: async_channel::Receiver<MprisCommand>,
) {
    let weak = Rc::downgrade(controller);
    glib::spawn_future_local(async move {
        while let Ok(command) = receiver.recv().await {
            let Some(controller) = weak.upgrade() else {
                break;
            };
            controller.handle_mpris_command(command);
        }
    });
}

impl PlayerController {
    /// Recomputes the MPRIS mirror from current controller state and writes
    /// it into the shared `Arc<Mutex<mpris::MprisState>>` — see this
    /// module's doc comment for the full list of call sites and why they
    /// cover every real transition. `status` is passed in rather than read
    /// from anywhere: callers always already know it more directly (the
    /// `PlayerEvent::StateChanged` payload, or the status `play_track_id`/
    /// `reset_to_stopped` are about to put the player into) than this
    /// function could re-derive it.
    ///
    /// `can_next`/`can_prev` both mirror `queue.is_empty()`'s negation — the
    /// same granularity `PlayerBar::set_transport_enabled` already uses
    /// (see `play_from_view`): this app doesn't compute a finer "not at the
    /// first/last track" distinction anywhere else either, so MPRIS clients
    /// see exactly the same enabled/disabled transport state the on-screen
    /// buttons do, rather than inventing new semantics here.
    ///
    /// `shuffle`/`repeat` are read fresh from `Queue` on every call — always
    /// current, no separate tracking needed. `volume` comes from
    /// `PlayerController::volume` (see that field's doc comment for why:
    /// `Player` has no volume getter of its own). `art_url` is copied from
    /// the retained now-playing cache; `now_playing_wiring.rs` fills it and
    /// patches the mirror after asynchronous cover resolution completes.
    ///
    /// `position_ms` is the one field this function does *not* simply
    /// re-derive: a status/track transition (this function's only trigger)
    /// isn't necessarily a position reset — pausing/resuming the *same*
    /// track must preserve the mirror's current position, not zero it. Only
    /// an actual track change (or dropping to no track at all) resets it —
    /// determined here by comparing the *previous* mirror's `track_id`
    /// against the new one before overwriting. Ordinary position updates
    /// (the 500 ms tick, a seek) go through `update_mpris_position` instead,
    /// entirely independently of this function.
    ///
    /// Borrow discipline: every `queue.borrow()` here is read and dropped
    /// inside its own statement; nothing after any of them calls back into
    /// `queue`, `player`, or GTK, so there's no re-entrancy hazard here the
    /// way there is at the other call sites documented in this module's
    /// `## Queue borrow discipline` section — the shape is kept consistent
    /// anyway.
    pub(super) fn update_mpris_mirror(&self, status: MprisPlaybackStatus) {
        self.update_agent_queue_mirror();
        let queue_has_tracks = !self.queue.borrow().is_empty()
            || !self.up_next.borrow().is_empty()
            || self.current_up_next.get().is_some();
        let is_shuffled = self.queue.borrow().is_shuffled();
        let repeat = self.queue.borrow().repeat();
        let now_playing = self.now_playing.borrow().clone();
        let volume = self.volume.get();

        let new_track_id = now_playing.as_ref().map(|track| track.id);

        let mut mirror = self
            .mpris_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let position_ms = if mirror.track_id == new_track_id {
            mirror.position_ms
        } else {
            0
        };

        *mirror = match now_playing {
            Some(track) => MprisState {
                status,
                track_id: Some(track.id),
                title: track.title,
                artist: track.artist,
                album: track.album,
                art_url: track.art_url,
                duration_ms: track.duration_ms,
                can_next: queue_has_tracks,
                can_prev: queue_has_tracks,
                position_ms,
                shuffle: is_shuffled,
                repeat,
                volume,
            },
            None => MprisState {
                status,
                track_id: None,
                title: String::new(),
                artist: String::new(),
                album: String::new(),
                art_url: None,
                duration_ms: 0,
                can_next: queue_has_tracks,
                can_prev: queue_has_tracks,
                position_ms,
                shuffle: is_shuffled,
                repeat,
                volume,
            },
        };
    }

    /// Refreshes the bounded queue mirror consumed by the local agent D-Bus
    /// interface. Values are cloned before locking so no `RefCell` borrow is
    /// held across the cross-thread mutex boundary.
    pub(super) fn update_agent_queue_mirror(&self) {
        let current_track_id = self.now_playing.borrow().as_ref().map(|track| track.id);
        let play_next_total = self.up_next.borrow().len();
        let play_next_track_ids = self
            .up_next
            .borrow()
            .ids()
            .iter()
            .take(AGENT_QUEUE_WINDOW)
            .copied()
            .collect();
        let context_total = self.queue.borrow().remaining_len();
        let context_track_ids = self.queue.borrow().remaining_window(0, AGENT_QUEUE_WINDOW);

        let mut mirror = self
            .agent_queue_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *mirror = media_integration::AgentQueueState {
            current_track_id,
            play_next_track_ids,
            context_track_ids,
            play_next_total,
            context_total,
        };
    }

    /// Patches only `position_ms` in the shared mirror — called by
    /// `apply_event`'s `PlayerEvent::Position` arm (every ~500 ms while
    /// playing) and by `seek` (immediately after a successful seek, so
    /// `Position` reflects it right away rather than waiting for the next
    /// tick). Deliberately doesn't touch any other field the way `update_
    /// mpris_mirror`'s full rebuild does, and — like every write to
    /// `position_ms` — triggers no `PropertiesChanged` of its own: `Position`
    /// is exempt from that per the MPRIS spec (see `mpris::MprisState`'s doc
    /// comment); `Seeked`, not a property notification, is how clients learn
    /// of jumps (see `notify_mpris_seek`).
    pub(super) fn update_mpris_position(&self, position_ms: i64) {
        let mut mirror = self
            .mpris_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        mirror.position_ms = position_ms;
    }

    /// Patches only `volume` in the shared mirror — called immediately after
    /// either the bar's volume control or an MPRIS `Volume` write changes
    /// `PlayerController::volume`, so the mirror (and thus MPRIS clients,
    /// once the next poll tick diffs it) reflects the new value without
    /// waiting for an unrelated status/track transition to refresh it via
    /// `update_mpris_mirror`.
    pub(super) fn update_mpris_volume(&self, volume: f64) {
        let mut mirror = self
            .mpris_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        mirror.volume = volume;
    }

    /// Patches only `shuffle` in the shared mirror — same immediacy
    /// rationale as `update_mpris_volume`, called after every `Queue::set_
    /// shuffle` regardless of whether the bar or MPRIS triggered it.
    pub(super) fn update_mpris_shuffle(&self, shuffle: bool) {
        let mut mirror = self
            .mpris_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        mirror.shuffle = shuffle;
    }

    /// Patches only `repeat` in the shared mirror — same immediacy rationale
    /// as `update_mpris_volume`, called after every `Queue::set_repeat`
    /// regardless of whether the bar or MPRIS triggered it.
    pub(super) fn update_mpris_repeat(&self, repeat: Repeat) {
        let mut mirror = self
            .mpris_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        mirror.repeat = repeat;
    }

    /// Seeks to `position_ms` — the one method behind every seek in the app,
    /// whatever originated it: the bar's seek scale (`player_controller_
    /// wiring.rs`'s `wire_bar_controls`/`connect_seek` closure) and every
    /// MPRIS-initiated seek (`Seek`/`SetPosition`, resolved to an absolute
    /// `position_ms` by `handle_mpris_command` above before calling this)
    /// all funnel through here (Stage 3 Task 10). One method for both
    /// origins so the `Seeked` signal — which the MPRIS spec requires after
    /// *every* successful seek, not just ones MPRIS itself initiated (the
    /// task brief is explicit: "auch app-internen!") — only has to be wired
    /// in one place (`notify_mpris_seek`, called from here) instead of at
    /// every seek call site individually. `pub(super)` so `player_
    /// controller_wiring.rs` can call it too. Defined here (rather than in
    /// `player_controller.rs`) since it exists entirely in service of the
    /// MPRIS `Seeked` story and both helpers it calls already live in this
    /// file.
    pub(super) fn seek(&self, position_ms: i64) {
        match self.player.seek_to(position_ms) {
            Ok(()) => {
                self.update_mpris_position(position_ms);
                self.notify_mpris_seek(position_ms);
                self.lyrics.external_seek(position_ms);
            }
            Err(error) => {
                tracing::error!(%error, position_ms, "seek failed");
            }
        }
    }

    /// Tells `mpris.rs`'s dedicated relay thread to emit the `Seeked` signal
    /// for `position_ms` (converted to µs, MPRIS's unit, via `mpris::ms_to_
    /// micros`) — called by `seek` after every successful seek, whichever
    /// side originated it (see `seek`'s doc comment above).
    /// `try_send` on an unbounded channel only fails once the relay
    /// thread is gone (app teardown) — logged, not propagated, matching
    /// every other MPRIS-bound `try_send` in this codebase (see `mpris.rs`'s
    /// `MprisPlayer::dispatch`).
    pub(super) fn notify_mpris_seek(&self, position_ms: i64) {
        let position_us = media_integration::ms_to_micros(position_ms);
        if let Err(error) = self.mpris_seek_notify.try_send(position_us) {
            tracing::warn!(
                %error,
                position_ms,
                "MPRIS Seeked notification dropped: relay thread is gone"
            );
        }
    }

    /// Dispatches one command received from `mpris.rs`'s D-Bus thread (see
    /// `player_controller.rs`'s `## MPRIS` doc section) — the MPRIS drain
    /// loop's only caller. `Stop` maps directly to `reset_to_stopped` (MPRIS
    /// has no weaker "pause and forget position" stop semantics to preserve
    /// here); `Next`/`Previous` map directly to the same named methods the
    /// bar buttons call. `Play`/`Pause`/`PlayPause` need their own small
    /// handling — see `mpris_play`/`mpris_pause`'s doc comments for why they
    /// aren't just `toggle_pause`. `SetPosition` is already fully resolved
    /// (trackid-matched, µs→ms converted, clamped) by `mpris.rs`'s `set_
    /// position` before it ever reaches here, so it goes straight to `seek`
    /// — the same method `Seek` (via `mpris_seek_relative`) and the bar's
    /// seek scale all funnel through. `PlayTrackIds` goes straight to
    /// `play_from_view` — see that arm's own comment below.
    pub(super) fn handle_mpris_command(&self, command: MprisCommand) {
        match command {
            MprisCommand::Play => self.mpris_play(),
            MprisCommand::Pause => self.mpris_pause(),
            MprisCommand::PlayPause => self.toggle_pause(),
            MprisCommand::Stop => self.reset_to_stopped(),
            MprisCommand::Next => self.next(),
            MprisCommand::Previous => self.previous(),
            MprisCommand::Seek(offset_ms) => self.mpris_seek_relative(offset_ms),
            MprisCommand::SetPosition(position_ms) => self.seek(position_ms),
            MprisCommand::SetShuffle(on) => self.mpris_set_shuffle(on),
            MprisCommand::SetLoop(repeat) => self.mpris_set_loop(repeat),
            MprisCommand::SetVolume(volume) => self.mpris_set_volume(volume),
            // Seeds the queue from `ids` and starts playback at index 0 —
            // the same `play_from_view` primitive the sidebar/track-list/
            // file-open call sites use (see that method's doc comment).
            // Origin is always `library()`: an MCP/D-Bus-issued play has no
            // browser view to attribute the context to, so it collapses to
            // the same fallback `file_open.rs`'s desktop-association path
            // uses for the same reason.
            MprisCommand::PlayTrackIds(ids) => self.play_from_view(
                ids,
                0,
                crate::ui::playback::play_origin::PlayOrigin::library(),
            ),
            MprisCommand::QueueAddNext(ids) => self.play_next(&ids),
            MprisCommand::QueueAddLast(ids) => self.append_to_queue(&ids),
            MprisCommand::QueueClear => self.clear_play_next(),
        }
    }

    /// MPRIS `Play`: per spec, starts or resumes playback — unlike
    /// `PlayPause`, it must not stop an already-playing track. Reads the
    /// current status from the MPRIS mirror (kept current by `update_mpris_
    /// mirror`) rather than adding a new `Player` query method purely for
    /// this: paused resumes via `toggle_pause`; stopped starts the queue's
    /// current track via `play_track_id`, if there is one; already playing
    /// is a no-op.
    fn mpris_play(&self) {
        match self.mpris_status() {
            MprisPlaybackStatus::Playing => {}
            MprisPlaybackStatus::Paused => self.toggle_pause(),
            MprisPlaybackStatus::Stopped => {
                let current = self
                    .current_up_next
                    .get()
                    .or_else(|| self.queue.borrow().current());
                match current {
                    Some(id) => self.play_track_id_with_change(
                        id,
                        crate::ui::current_track_selection::CurrentTrackChange::ExplicitTransport,
                    ),
                    None if !self.up_next.borrow().is_empty() => {
                        self.advance_playback(crate::ui::up_next_transport::AdvanceReason::Manual);
                    }
                    None => {
                        tracing::debug!("MPRIS Play: queue is empty; nothing to play");
                    }
                }
            }
        }
    }

    /// MPRIS `Pause`: per spec, pauses — a no-op unless currently playing
    /// (unlike `PlayPause`, must not *resume* a paused track). See `mpris_
    /// play`'s doc comment for why this reads the mirror rather than adding
    /// a new `Player` query method.
    fn mpris_pause(&self) {
        if self.mpris_status() == MprisPlaybackStatus::Playing {
            self.toggle_pause();
        }
    }

    /// The MPRIS mirror's current `status` field — poisoned-recovery lock,
    /// same pattern `player.rs` uses everywhere it locks a mutex.
    fn mpris_status(&self) -> MprisPlaybackStatus {
        self.mpris_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .status
    }

    /// MPRIS `Seek(offset_µs)` (already converted to `offset_ms` by `mpris.
    /// rs`'s `set_position`/`seek` boundary — see `MprisCommand`'s doc
    /// comment): resolves the *relative* offset against the mirror's current
    /// `position_ms` into an *absolute* target, clamped to `0..=duration_ms`
    /// (no upper clamp if `duration_ms` isn't known, i.e. `<= 0`), then hands
    /// off to `seek` — the same method the bar's seek scale and MPRIS's
    /// `SetPosition` both use.
    fn mpris_seek_relative(&self, offset_ms: i64) {
        let snapshot = self
            .mpris_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();

        let mut target_ms = (snapshot.position_ms + offset_ms).max(0);
        if snapshot.duration_ms > 0 {
            target_ms = target_ms.min(snapshot.duration_ms);
        }
        self.seek(target_ms);
    }

    /// MPRIS `Shuffle` write: applies it to the queue, then syncs both the
    /// bar's shuffle toggle (guarded against re-dispatching — see `ui::
    /// player_bar`'s `set_shuffle_indicator`) and the mirror (immediately,
    /// via `update_mpris_shuffle` — see that method's doc comment for why,
    /// rather than waiting for the next unrelated `update_mpris_mirror`
    /// call) so a client reading `Shuffle` right back sees its own write
    /// reflected. Borrow discipline: `set_shuffle`/`is_shuffled` each run in
    /// their own statement — see the module's `## Queue borrow discipline`
    /// doc section.
    fn mpris_set_shuffle(&self, on: bool) {
        self.queue.borrow_mut().set_shuffle(on);
        let is_shuffled = self.queue.borrow().is_shuffled();
        // Syncs the bar AND the Now-Playing page — see `now_playing_wiring.
        // rs`'s `sync_shuffle_indicator` doc comment.
        self.sync_shuffle_indicator(is_shuffled);
        self.update_mpris_shuffle(is_shuffled);
        tracing::debug!(is_shuffled, "MPRIS: shuffle set");
    }

    /// MPRIS `LoopStatus` write (already parsed to `Repeat` by `mpris.rs`'s
    /// `set_loop_status` — invalid strings never reach here, see that
    /// method's doc comment): applies it to the queue, then syncs both the
    /// bar's repeat button and the mirror, same shape as `mpris_set_
    /// shuffle`. The repeat button needs no reentrancy guard (unlike
    /// shuffle/volume) — `PlayerBar::set_repeat_indicator` only ever swaps
    /// an icon/CSS class, which cannot itself fire the button's `clicked`
    /// signal the way `ToggleButton::set_active`/`ScaleButton::set_value` do.
    fn mpris_set_loop(&self, repeat: Repeat) {
        self.queue.borrow_mut().set_repeat(repeat);
        self.sync_repeat_indicator(repeat);
        self.update_mpris_repeat(repeat);
        tracing::debug!(?repeat, "MPRIS: loop status set");
    }

    /// MPRIS `Volume` write (already clamped to `0.0..=1.0` by `mpris.rs`'s
    /// `set_volume` — see that method's doc comment): applies it to the
    /// player, the tracked `volume` field (`Player` has no getter of its
    /// own — see that field's doc comment in `player_controller.rs`), the
    /// bar's volume control (guarded — see `ui::player_bar`'s `set_volume_
    /// indicator`), and the mirror, same immediacy shape as `mpris_set_
    /// shuffle`/`mpris_set_loop`.
    fn mpris_set_volume(&self, volume: f64) {
        self.player.set_volume(volume);
        self.volume.set(volume);
        self.sync_volume_indicator(volume);
        self.update_mpris_volume(volume);
        tracing::debug!(volume, "MPRIS: volume set");
    }
}

/// Maps `player::PlaybackState` to `mpris::MprisPlaybackStatus` — the one
/// explicit conversion between the two (see this module's doc comment for
/// why they're deliberately separate types). Pure, so it's unit-testable
/// directly like `cycle_repeat`/`should_stop_skipping` in the other two
/// modules.
pub(super) fn mpris_status_from_playback_state(state: PlaybackState) -> MprisPlaybackStatus {
    match state {
        PlaybackState::Playing => MprisPlaybackStatus::Playing,
        PlaybackState::Paused => MprisPlaybackStatus::Paused,
        PlaybackState::Stopped => MprisPlaybackStatus::Stopped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mpris_status_mirrors_playback_state() {
        assert_eq!(
            mpris_status_from_playback_state(PlaybackState::Playing),
            MprisPlaybackStatus::Playing
        );
        assert_eq!(
            mpris_status_from_playback_state(PlaybackState::Paused),
            MprisPlaybackStatus::Paused
        );
        assert_eq!(
            mpris_status_from_playback_state(PlaybackState::Stopped),
            MprisPlaybackStatus::Stopped
        );
    }
}
