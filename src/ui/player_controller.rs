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

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib;
use rusqlite::Connection;

use crate::library::stats;
use crate::models::Track;
use crate::player::{PlaybackState, Player, PlayerError, PlayerEvent};
use crate::ui::player_bar::PlayerBar;

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
    /// `play_track` from the activated row's `Track` and cleared once play
    /// tracking has been evaluated for it (see `evaluate_play_tracking`).
    /// `None` when no track is loaded.
    current_track: Cell<Option<(i64, i64)>>,
    /// The highest playback position observed for `current_track` via
    /// `Position` events — not the most recent one, so seeking backward
    /// near the end of a track can't cost a listener credit for having
    /// already passed the 50% mark. Reset to 0 whenever a new track starts.
    max_position_ms: Cell<i64>,
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
        });

        wire_bar_controls(&controller);

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

    /// Starts playback of `track` (row activation lands here). The bar's
    /// title/artist come straight from the activated row's `Track` — no
    /// extra DB query. If another track is already loaded, its play
    /// tracking is evaluated first (a switch, not a `TrackFinished`/`Stop`,
    /// is still the end of that track's listening session). On failure to
    /// start the new track: log and reset to stopped; the app keeps running
    /// (fault-tolerance rule — a missing/corrupt file must never crash or
    /// wedge the UI).
    pub fn play_track(&self, track: &Track) {
        self.evaluate_play_tracking();
        self.current_track.set(Some((track.id, track.duration_ms)));
        self.max_position_ms.set(0);

        self.bar.set_track(&track.title, &track.artist);
        if let Err(error) = self.player.play(&track.path) {
            tracing::error!(%error, path = %track.path, "failed to start playback");
            self.reset_to_stopped();
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
                // Queue/auto-advance logic is a later stage; for now the end
                // of a track simply returns the bar to its empty state.
                tracing::info!("track finished: resetting player to stopped");
                self.reset_to_stopped();
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
    /// call it for those cases (`play_track` calls it separately, for the
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
