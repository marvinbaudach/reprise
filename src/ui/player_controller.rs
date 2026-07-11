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

use std::rc::Rc;

use gtk4::glib;

use crate::models::Track;
use crate::player::{PlaybackState, Player, PlayerError, PlayerEvent};
use crate::ui::player_bar::PlayerBar;

/// Owns the `Player` and its `PlayerBar`, routing user input from the bar to
/// the player and `PlayerEvent`s from the player back onto the bar (on the
/// GTK main thread — see the module doc comment).
pub struct PlayerController {
    player: Player,
    bar: PlayerBar,
}

impl PlayerController {
    /// Builds the player, the bar, and the event bridge between them.
    /// Returns `Err` if GStreamer is unavailable (no playbin, bad
    /// `REPRISE_AUDIO_SINK` override, …) — the caller decides how to degrade
    /// (see `window::build`: library browsing keeps working without a bar).
    pub fn new() -> Result<Rc<Self>, PlayerError> {
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
    /// extra DB query. On failure: log and reset to stopped; the app keeps
    /// running (fault-tolerance rule — a missing/corrupt file must never
    /// crash or wedge the UI).
    pub fn play_track(&self, track: &Track) {
        self.bar.set_track(&track.title, &track.artist);
        if let Err(error) = self.player.play(&track.path) {
            tracing::error!(%error, path = %track.path, "failed to start playback");
            self.reset_to_stopped();
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
    /// state. On success this relies entirely on the `StateChanged(Stopped)`
    /// event `stop()` emits — routed back here through `apply_event` — so
    /// the bar isn't reset twice. If `stop()` itself fails, though, that
    /// event never fires, so the bar is reset directly right here instead:
    /// the UI must still land in a consistent stopped state even when
    /// stopping the pipeline errors out.
    fn reset_to_stopped(&self) {
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
