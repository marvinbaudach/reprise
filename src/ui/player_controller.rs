//! Bridges `Player` (whose events fire on GStreamer bus-watch and ticker
//! threads) to the GTK main thread and the `PlayerBar` widgets.
//!
//! ## Event marshalling: `async-channel` + `glib::spawn_future_local`
//!
//! Every GTK widget call must happen on the main thread, but `Player::new`'s
//! callback is invoked from non-GTK threads. The bridge used here is the
//! pattern the gtk4-rs book recommends: the player callback does a
//! non-blocking `try_send` into an unbounded `async_channel`, and a single
//! future spawned on the default (main) `glib::MainContext` drains the
//! receiver and applies each event to the bar. Compared to the alternative —
//! `glib::idle_add` from the callback thread per event — this keeps one
//! long-lived drain loop with strict FIFO ordering instead of allocating a
//! GSource per event, and it makes the thread boundary explicit in the
//! types: only `PlayerEvent` (plain `Send` data) crosses it, exactly the
//! runtime-safety property the player was designed around.
//!
//! ## Lifetime
//!
//! The drain future holds only a `Weak` reference to the controller, so the
//! controller's lifetime stays tied to the window (via the track-list
//! activation closure that owns the `Rc`), not to the future. Once the
//! controller drops, the next event breaks the loop, the receiver closes,
//! and the player callback's `try_send` starts failing (logged at warn —
//! that only happens during teardown).
//!
//! ## Play tracking
//!
//! `current_track`/`max_position_ms` track, per loaded track, the furthest
//! playback position observed via `Position` events. Whenever a listening
//! session ends — the track finishes, playback switches to a different
//! track, or the player is reset to stopped after an error — an idempotent
//! `evaluate_play_tracking` call checks `library::stats::should_count_play`
//! against that high-water mark and `library::stats::record_play`s a play if
//! it crosses the 50%-listened threshold, then clears the tracked state.
//!
//! ## Queue borrow discipline
//!
//! `queue` is a `RefCell<Queue>` (Stage 2 Task 3/4). Two prior bugs in this
//! codebase — a `RefCell` borrow held across a re-entrant GTK callback in
//! `player_bar.rs`'s seek scale, and the same class of bug in
//! `ui::rating::RatingWidget` (see that module's doc comment) — were both
//! "hold a `Ref`/`RefMut` across a call that can synchronously call back
//! into the same object". `play_track_id` (the one function that actually
//! starts playback) can synchronously trigger `PlayerEvent`s and eventually
//! another `apply_event`/queue call, so **no `queue` borrow may still be
//! alive when `play_track_id`, `reset_to_stopped`, or any other
//! player/GTK-facing call runs.** Every call site here follows the same
//! shape: read the one `Option<i64>`/value it needs out of the queue in a
//! single expression (a `let x = queue.borrow_mut().method();` statement, or
//! an explicit `{ }` block when more than one queue call is needed), which
//! drops the borrow at the end of that statement/block — *before* the next
//! statement calls out. Unlike the two prior bugs, no explicit `Rc::clone`-
//! out step is needed here: `Queue`'s methods return owned `Option<i64>`/
//! `Repeat` values, not references into the `RefCell`'s contents, so the
//! borrow's temporary scope is inherently the only thing to manage.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib;
use rusqlite::Connection;

use crate::library::stats;
use crate::player::{PlaybackState, Player, PlayerError, PlayerEvent};
use crate::queries;
use crate::queue::{Queue, Repeat};
use crate::ui::player_bar::PlayerBar;

/// Dev/verification hook (permanent, like `REPRISE_SCAN_DIR`/`REPRISE_
/// SMOKE_QUIT`/`REPRISE_SMOKE_ACTIVATE`): when set to `"all"`, forces
/// `Repeat::All` right after the controller (and its queue) are built, so a
/// headless E2E can observe auto-advance wrapping from the last track back
/// to the first without a human toggling the repeat button.
///
/// Usage: `REPRISE_SMOKE_REPEAT=all REPRISE_SCAN_DIR=… REPRISE_SMOKE_ACTIVATE=1
///  REPRISE_AUDIO_SINK=fakesink REPRISE_SMOKE_QUIT=1 xvfb-run -a cargo run`.
const SMOKE_REPEAT_ENV_VAR: &str = "REPRISE_SMOKE_REPEAT";
const SMOKE_REPEAT_ALL_VALUE: &str = "all";

/// Owns the `Player` and its `PlayerBar`, routing user input from the bar to
/// the player and `PlayerEvent`s from the player back onto the bar (on the
/// GTK main thread — see the module doc comment). Also owns play-count
/// tracking (see the module's `## Play tracking` section below).
pub struct PlayerController {
    player: Player,
    bar: PlayerBar,
    /// The UI-owned database connection, shared with `track_list.rs` (see
    /// `window::build`) — used only to write play-count updates via
    /// `library::stats::record_play`.
    conn: Rc<RefCell<Connection>>,
    /// `(track_id, duration_ms)` of the track currently loaded, set by
    /// `play_track_id` and cleared once play tracking has been evaluated for
    /// it (see `evaluate_play_tracking`). `None` when no track is loaded.
    current_track: Cell<Option<(i64, i64)>>,
    /// The highest playback position observed for `current_track` via
    /// `Position` events — not the most recent one, so seeking backward
    /// near the end of a track can't cost a listener credit for having
    /// already passed the 50% mark. Reset to 0 whenever a new track starts.
    max_position_ms: Cell<i64>,
    /// The playback queue (Stage 2 Task 3/4): track order, shuffle, and
    /// repeat mode. `play_from_view` seeds it; `TrackFinished`/the
    /// previous/next buttons step through it. See the module's `## Queue
    /// borrow discipline` doc section for the rule every call site here
    /// follows.
    queue: RefCell<Queue>,
}

impl PlayerController {
    /// Builds the player, the bar, and the event bridge between them.
    /// `conn` is the same UI-owned database connection `track_list.rs`
    /// holds, used to record plays. Returns `Err` if GStreamer is
    /// unavailable (no playbin, bad `REPRISE_AUDIO_SINK` override, …) — the
    /// caller decides how to degrade (see `window::build`: library browsing
    /// keeps working without a bar).
    pub fn new(conn: Rc<RefCell<Connection>>) -> Result<Rc<Self>, PlayerError> {
        let (sender, receiver) = async_channel::unbounded::<PlayerEvent>();

        let player = Player::new(Box::new(move |event| {
            if let Err(error) = sender.try_send(event) {
                // Unbounded channel: try_send only fails once the receiver
                // (the drain loop below) is gone, i.e. during app teardown.
                tracing::warn!(%error, "player event dropped: UI receiver is gone");
            }
        }))?;

        let controller = Rc::new(Self {
            player,
            bar: PlayerBar::new(),
            conn,
            current_track: Cell::new(None),
            max_position_ms: Cell::new(0),
            queue: RefCell::new(Queue::new()),
        });

        wire_bar_controls(&controller);
        arm_smoke_repeat(&controller);

        let weak = Rc::downgrade(&controller);
        glib::spawn_future_local(async move {
            while let Ok(event) = receiver.recv().await {
                let Some(controller) = weak.upgrade() else {
                    break;
                };
                controller.apply_event(event);
            }
        });

        Ok(controller)
    }

    /// The bottom-bar widget for `ToolbarView::add_bottom_bar`.
    pub fn bar_widget(&self) -> &gtk4::ActionBar {
        self.bar.widget()
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
    pub fn play_from_view(&self, ids: Vec<i64>, start_index: usize) {
        self.queue.borrow_mut().set_tracks(ids, start_index);

        let queue_len = self.queue.borrow().len();
        tracing::info!(queue_len, start_index, "queue set from view");
        self.bar
            .set_transport_enabled(!self.queue.borrow().is_empty());

        let current = self.queue.borrow().current();
        match current {
            Some(id) => self.play_track_id(id),
            None => self.reset_to_stopped(),
        }
    }

    /// Resolves `id` via `queries::query_track_summary` and starts its
    /// playback — the one place that actually calls `Player::play`, shared
    /// by `play_from_view` and every queue-stepping call site
    /// (`TrackFinished`'s auto-advance, the previous/next buttons) so the
    /// "resolve, evaluate prior play tracking, start playback, handle
    /// failure" sequence exists exactly once (DRY). Ends the previous
    /// track's listening session first (`evaluate_play_tracking`, same as
    /// the old per-`Track` `play_track` did) — a queue step is still a
    /// track switch. On a missing row or a playback failure: log and reset
    /// to stopped rather than leaving the bar/pipeline in an inconsistent
    /// state (fault tolerance — a missing/corrupt file must never crash or
    /// wedge the UI, and now also must never silently stall the queue).
    fn play_track_id(&self, id: i64) {
        self.evaluate_play_tracking();

        let summary = {
            let conn = self.conn.borrow();
            queries::query_track_summary(&conn, id)
        };

        match summary {
            Ok(Some(summary)) => {
                self.current_track.set(Some((id, summary.duration_ms)));
                self.max_position_ms.set(0);

                self.bar.set_track(&summary.title, &summary.artist);
                if let Err(error) = self.player.play(&summary.path) {
                    tracing::error!(%error, path = %summary.path, track_id = id, "failed to start playback");
                    self.reset_to_stopped();
                }
            }
            Ok(None) => {
                tracing::warn!(
                    track_id = id,
                    "queue: track id has no matching database row; skipping playback"
                );
                self.reset_to_stopped();
            }
            Err(error) => {
                tracing::error!(%error, track_id = id, "failed to resolve track for playback");
                self.reset_to_stopped();
            }
        }
    }

    /// Evaluates whether the currently loaded track (if any) crossed the
    /// 50%-listened threshold (`library::stats::should_count_play`) and, if
    /// so, records a play — then clears the tracked state either way.
    /// Idempotent: called with no current track, it's a no-op, so every
    /// call site below (track switch, finish, error/stop) can call it
    /// unconditionally without risking a double-count.
    fn evaluate_play_tracking(&self) {
        let Some((track_id, duration_ms)) = self.current_track.take() else {
            return;
        };
        let max_position_ms = self.max_position_ms.replace(0);

        if !stats::should_count_play(max_position_ms, duration_ms) {
            return;
        }

        let conn = self.conn.borrow();
        match stats::record_play(&conn, track_id, now_unix()) {
            Ok(()) => {
                tracing::debug!(track_id, max_position_ms, duration_ms, "play recorded");
            }
            Err(error) => {
                tracing::error!(%error, track_id, "failed to record play");
            }
        }
    }

    /// Applies one marshalled `PlayerEvent` to the bar. Runs on the GTK main
    /// thread (called only from the drain loop in `new`).
    fn apply_event(&self, event: PlayerEvent) {
        match event {
            PlayerEvent::StateChanged(state) => {
                tracing::info!(?state, "player bar: applying state change");
                self.bar.set_state(state);
                if state == PlaybackState::Stopped {
                    self.bar.clear_track();
                }
            }
            PlayerEvent::Position {
                position_ms,
                duration_ms,
            } => {
                // Debug, not info: fires every 500 ms during playback.
                tracing::debug!(
                    position_ms,
                    duration_ms,
                    "player bar: applying position tick"
                );
                self.max_position_ms
                    .set(self.max_position_ms.get().max(position_ms));
                self.bar.set_position(position_ms, duration_ms);
            }
            PlayerEvent::TrackFinished => {
                tracing::info!("track finished: advancing queue");
                // Borrow discipline: `advance_auto()` runs inside this one
                // `let` statement, so the `queue` borrow drops before
                // `play_track_id`/`reset_to_stopped` run below — see the
                // module's `## Queue borrow discipline` doc section.
                let next = self.queue.borrow_mut().advance_auto();
                match next {
                    Some(id) => self.play_track_id(id),
                    None => {
                        tracing::info!("queue exhausted: resetting player to stopped");
                        self.reset_to_stopped();
                    }
                }
            }
            PlayerEvent::Error(message) => {
                tracing::error!(%message, "player error: resetting player to stopped");
                self.reset_to_stopped();
            }
        }
    }

    /// Stops the pipeline and ensures the bar lands in the stopped/empty
    /// state. Evaluates play tracking for whatever track was loaded first
    /// (see `evaluate_play_tracking`) — every path that ends a listening
    /// session (`TrackFinished`, a player error, and a future explicit
    /// stop) funnels through here, so this is the one place that needs to
    /// call it for those cases (`play_track_id` calls it separately, for the
    /// track-switch case, since that path never calls `reset_to_stopped`).
    /// On success the rest of this relies entirely on the
    /// `StateChanged(Stopped)` event `stop()` emits — routed back here
    /// through `apply_event` — so the bar isn't reset twice. If `stop()`
    /// itself fails, though, that event never fires, so the bar is reset
    /// directly right here instead: the UI must still land in a consistent
    /// stopped state even when stopping the pipeline errors out.
    fn reset_to_stopped(&self) {
        self.evaluate_play_tracking();
        match self.player.stop() {
            Ok(()) => {}
            Err(error) => {
                tracing::error!(%error, "failed to stop player during reset");
                self.bar.set_state(PlaybackState::Stopped);
                self.bar.clear_track();
            }
        }
    }
}

/// Wires the bar's user-input signals to player calls. Each closure holds a
/// `Weak` controller reference: the bar is owned *by* the controller, so a
/// strong reference here would be a leak-guaranteeing Rc cycle.
fn wire_bar_controls(controller: &Rc<PlayerController>) {
    let weak = Rc::downgrade(controller);
    controller.bar.connect_play_pause(move || {
        let Some(controller) = weak.upgrade() else {
            return;
        };
        if let Err(error) = controller.player.toggle_pause() {
            tracing::error!(%error, "toggle play/pause failed");
        }
    });

    let weak = Rc::downgrade(controller);
    controller.bar.connect_seek(move |position_ms| {
        let Some(controller) = weak.upgrade() else {
            return;
        };
        if let Err(error) = controller.player.seek_to(position_ms) {
            tracing::error!(%error, position_ms, "seek failed");
        }
    });

    let weak = Rc::downgrade(controller);
    controller.bar.connect_volume_changed(move |volume| {
        let Some(controller) = weak.upgrade() else {
            return;
        };
        controller.player.set_volume(volume);
    });

    let weak = Rc::downgrade(controller);
    controller.bar.connect_previous(move || {
        let Some(controller) = weak.upgrade() else {
            return;
        };
        // Borrow discipline: see the module's `## Queue borrow discipline`
        // doc section — `previous()` runs inside this one `let` statement,
        // so the borrow drops before `play_track_id`/`reset_to_stopped`.
        let previous = controller.queue.borrow_mut().previous();
        match previous {
            Some(id) => controller.play_track_id(id),
            None => controller.reset_to_stopped(),
        }
    });

    let weak = Rc::downgrade(controller);
    controller.bar.connect_next(move || {
        let Some(controller) = weak.upgrade() else {
            return;
        };
        let next = controller.queue.borrow_mut().next_manual();
        match next {
            Some(id) => controller.play_track_id(id),
            None => controller.reset_to_stopped(),
        }
    });

    let weak = Rc::downgrade(controller);
    controller.bar.connect_shuffle_toggled(move |active| {
        let Some(controller) = weak.upgrade() else {
            return;
        };
        controller.queue.borrow_mut().set_shuffle(active);
        // Read back the queue's own idea of shuffle state (rather than just
        // logging `active`) so a log line always reflects what `Queue`
        // actually did, not just what the button asked for.
        let is_shuffled = controller.queue.borrow().is_shuffled();
        tracing::debug!(is_shuffled, "shuffle toggled");
    });

    let weak = Rc::downgrade(controller);
    controller.bar.connect_repeat_clicked(move || {
        let Some(controller) = weak.upgrade() else {
            return;
        };
        // Explicit block (not a single statement): reading the current mode
        // and setting the new one both need the same borrow, so they're
        // scoped together here — still dropped before `set_repeat_
        // indicator` (a GTK call) runs after the block. See the module's
        // `## Queue borrow discipline` doc section.
        let next_repeat = {
            let mut queue = controller.queue.borrow_mut();
            let next_repeat = cycle_repeat(queue.repeat());
            queue.set_repeat(next_repeat);
            next_repeat
        };
        controller.bar.set_repeat_indicator(next_repeat);
    });
}

/// Cycles the repeat mode in the mockup's button order: Off -> All -> One ->
/// Off. Pure (no `Queue`/GTK access) so it's unit-testable directly.
fn cycle_repeat(current: Repeat) -> Repeat {
    match current {
        Repeat::Off => Repeat::All,
        Repeat::All => Repeat::One,
        Repeat::One => Repeat::Off,
    }
}

/// Arms `REPRISE_SMOKE_REPEAT=all` (see the const's doc comment above):
/// forces the queue into `Repeat::All` right after construction and syncs
/// the bar's repeat indicator to match, so a headless E2E run can observe
/// auto-advance wrapping from the last queued track back to the first.
fn arm_smoke_repeat(controller: &Rc<PlayerController>) {
    let Ok(value) = std::env::var(SMOKE_REPEAT_ENV_VAR) else {
        return;
    };
    if value != SMOKE_REPEAT_ALL_VALUE {
        tracing::warn!(
            value,
            "{SMOKE_REPEAT_ENV_VAR} set to an unrecognized value; ignoring (expected \"{SMOKE_REPEAT_ALL_VALUE}\")"
        );
        return;
    }
    tracing::info!(
        "{SMOKE_REPEAT_ENV_VAR}={SMOKE_REPEAT_ALL_VALUE} set: forcing Repeat::All for headless wrap-around E2E"
    );
    controller.queue.borrow_mut().set_repeat(Repeat::All);
    controller.bar.set_repeat_indicator(Repeat::All);
}

/// Current time as Unix seconds, for `record_play`'s `last_played_at`.
/// Mirrors `library::scanner`'s private `now_unix` helper (not made public
/// there — see that module's doc comment) rather than sharing it, since the
/// two callers are otherwise unrelated and the function is a single
/// `SystemTime` call.
fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_repeat_goes_off_all_one_off() {
        assert_eq!(cycle_repeat(Repeat::Off), Repeat::All);
        assert_eq!(cycle_repeat(Repeat::All), Repeat::One);
        assert_eq!(cycle_repeat(Repeat::One), Repeat::Off);
    }
}
