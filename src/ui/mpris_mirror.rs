//! MPRIS mirror updates and command handling for `PlayerController` (Stage 2
//! Task 6; split out of `player_controller.rs` in Stage 3 Task 1 — see that
//! module's `## MPRIS` doc section for the parts of the MPRIS story that
//! stayed there: field ownership (`mpris_state`/`now_playing`), starting the
//! D-Bus thread in `PlayerController::new`, and the drain loop that calls
//! `handle_mpris_command` below once per received command).
//!
//! ## What lives here
//!
//! - `update_mpris_mirror`: recomputes `mpris::MprisState` from the
//!   controller's current playback state and writes it into the shared
//!   `Arc<Mutex<mpris::MprisState>>` the D-Bus thread reads.
//! - `handle_mpris_command`/`mpris_play`/`mpris_pause`/`mpris_status`: applies
//!   one `MprisCommand` received from that thread — the MPRIS drain loop's
//!   only caller.
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
//! marks exactly the fields (`queue`, `mpris_state`, `now_playing`) and
//! methods (`play_track_id`, `toggle_pause`, `next`, `previous`, `reset_to_
//! stopped`) this module touches as `pub(super)` — visible throughout `ui`
//! and its descendants, but no wider (deliberately not `pub(crate)`, let
//! alone `pub`: nothing outside `ui` needs any of this). `player_controller.
//! rs` still owns every field outright; this module (like `playback_faults.
//! rs`) only ever borrows `&self` via `impl PlayerController` blocks defined
//! here — inherent impls for one type are allowed to span multiple modules/
//! files within a crate, so no new type or wrapper is needed to split the
//! behavior out.
//!
//! ## Queue borrow discipline
//!
//! Same invariant as `player_controller.rs`'s `## Queue borrow discipline`
//! doc section — read it there for the full rationale (three prior review
//! catches motivate it): no `queue` `Ref`/`RefMut` may still be alive when a
//! call that can re-enter the player/GTK runs. `mpris_play` reads `queue.
//! borrow().current()` inside its own `let` statement, dropping the borrow
//! before the `self.play_track_id(id)` call below it. `update_mpris_mirror`
//! only ever reads `queue.borrow().is_empty()` inside its own statement and
//! never calls back out afterward, so it isn't actually a re-entrancy hazard
//! here — but the hoist-into-one-statement shape is kept consistent anyway.

use crate::mpris::{MprisCommand, MprisPlaybackStatus, MprisState};
use crate::player::PlaybackState;
use crate::ui::player_controller::PlayerController;

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
    /// Borrow discipline: `queue.borrow()` is read and dropped inside this
    /// one statement; nothing after it calls back into `queue`, `player`, or
    /// GTK, so there's no re-entrancy hazard here the way there is at the
    /// other call sites documented in this module's `## Queue borrow
    /// discipline` section — the shape is kept consistent anyway.
    pub(super) fn update_mpris_mirror(&self, status: MprisPlaybackStatus) {
        let queue_has_tracks = !self.queue.borrow().is_empty();
        let now_playing = self.now_playing.borrow().clone();

        let new_state = match now_playing {
            Some(track) => MprisState {
                status,
                track_id: Some(track.id),
                title: track.title,
                artist: track.artist,
                album: track.album,
                duration_ms: track.duration_ms,
                can_next: queue_has_tracks,
                can_prev: queue_has_tracks,
            },
            None => MprisState {
                status,
                track_id: None,
                title: String::new(),
                artist: String::new(),
                album: String::new(),
                duration_ms: 0,
                can_next: queue_has_tracks,
                can_prev: queue_has_tracks,
            },
        };

        let mut mirror = self
            .mpris_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *mirror = new_state;
    }

    /// Dispatches one command received from `mpris.rs`'s D-Bus thread (see
    /// `player_controller.rs`'s `## MPRIS` doc section) — the MPRIS drain
    /// loop's only caller. `Stop` maps directly to `reset_to_stopped` (MPRIS
    /// has no weaker "pause and forget position" stop semantics to preserve
    /// here); `Next`/`Previous` map directly to the same named methods the
    /// bar buttons call. `Play`/`Pause`/`PlayPause` need their own small
    /// handling — see `mpris_play`/`mpris_pause`'s doc comments for why they
    /// aren't just `toggle_pause`.
    pub(super) fn handle_mpris_command(&self, command: MprisCommand) {
        match command {
            MprisCommand::Play => self.mpris_play(),
            MprisCommand::Pause => self.mpris_pause(),
            MprisCommand::PlayPause => self.toggle_pause(),
            MprisCommand::Stop => self.reset_to_stopped(),
            MprisCommand::Next => self.next(),
            MprisCommand::Previous => self.previous(),
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
                let current = self.queue.borrow().current();
                match current {
                    Some(id) => self.play_track_id(id),
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
