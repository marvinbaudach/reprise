//! Player-event application and stopped-state recovery for PlayerController.

use std::rc::Rc;

use crate::ui::mpris_mirror::mpris_status_from_playback_state;
use crate::ui::player_controller::PlayerController;
use reprise_core::media_integration::MprisPlaybackStatus;
use reprise_core::playback::{PlaybackState, PlayerEvent, SpectrumFrame};

/// Prevent one busy drain iteration from starving the rest of GTK's main loop.
const MAX_PLAYER_EVENT_BURST: usize = 64;

/// Collapses consecutive analyzer frames without moving them across semantic
/// playback events. This keeps transport/state ordering exact while making
/// spectrum delivery latest-wins under GTK render load.
pub(super) fn coalesce_player_event_burst(
    events: impl IntoIterator<Item = PlayerEvent>,
) -> Vec<PlayerEvent> {
    let mut coalesced = Vec::new();
    let mut pending_spectrum: Option<SpectrumFrame> = None;

    for event in events {
        match event {
            PlayerEvent::Spectrum(frame) => {
                pending_spectrum = Some(match pending_spectrum {
                    Some(previous) => previous.coalesce_latest(frame),
                    None => frame,
                });
            }
            event => {
                if let Some(frame) = pending_spectrum.take() {
                    coalesced.push(PlayerEvent::Spectrum(frame));
                }
                coalesced.push(event);
            }
        }
    }
    if let Some(frame) = pending_spectrum {
        coalesced.push(PlayerEvent::Spectrum(frame));
    }
    coalesced
}

pub(super) fn spawn_event_drain(
    controller: &Rc<PlayerController>,
    receiver: async_channel::Receiver<PlayerEvent>,
) {
    let weak = Rc::downgrade(controller);
    gtk4::glib::spawn_future_local(async move {
        while let Ok(first_event) = receiver.recv().await {
            let Some(controller) = weak.upgrade() else {
                break;
            };
            let mut burst = Vec::with_capacity(MAX_PLAYER_EVENT_BURST);
            burst.push(first_event);
            while burst.len() < MAX_PLAYER_EVENT_BURST {
                let Ok(event) = receiver.try_recv() else {
                    break;
                };
                burst.push(event);
            }
            for event in coalesce_player_event_burst(burst) {
                controller.apply_event(event);
            }
        }
    });
}

impl PlayerController {
    /// Applies one marshalled `PlayerEvent` to the bar. Runs on the GTK main
    /// thread (called only from the drain loop in `new`).
    pub(in crate::ui) fn apply_event(self: &std::rc::Rc<Self>, event: PlayerEvent) {
        match event {
            PlayerEvent::StateChanged(state) => {
                tracing::info!(?state, "player bar: applying state change");
                match self.playback_mode() {
                    super::preview::PlaybackMode::Podcast | super::preview::PlaybackMode::Radio => {
                        self.external_state_changed(state);
                    }
                    super::preview::PlaybackMode::Queue | super::preview::PlaybackMode::Preview => {
                        self.sync_state(state);
                    }
                }
                if state == PlaybackState::Stopped {
                    // Defensive, before `sync_clear_track()` fans out the
                    // empty loaded-track snapshot:
                    // `reset_to_stopped` (the only caller of `Player::stop`)
                    // already clears `now_playing` itself before this event
                    // even has a chance to drain, but a stray `Stopped` from
                    // elsewhere must not leave stale metadata mirrored.
                    if self.playback_mode() == super::preview::PlaybackMode::Queue {
                        *self.now_playing.borrow_mut() = None;
                        self.sync_clear_track();
                    }
                    // Now-playing observers (the track table's + the Artists
                    // view's mini-EQ) are turned off via the
                    // `playback_state_changed(Stopped)` fan-out that
                    // `sync_state` above already fires — see
                    // `current_track_selection::wire`.
                }
                match self.playback_mode() {
                    super::preview::PlaybackMode::Podcast | super::preview::PlaybackMode::Radio => {
                    }
                    super::preview::PlaybackMode::Queue | super::preview::PlaybackMode::Preview => {
                        self.update_mpris_mirror(mpris_status_from_playback_state(state));
                    }
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
                self.handle_external_position(position_ms, duration_ms);
                self.sync_position(position_ms, duration_ms);
                // Stage 3 Task 10: keeps MPRIS's `Position` current between
                // `update_mpris_mirror` rebuilds — see `update_mpris_
                // position`'s doc comment.
                self.update_mpris_position(position_ms);
            }
            PlayerEvent::TrackFinished => {
                // INST-4b/5b: a finished instrumental preview stops without
                // advancing the queue (so a stale gapless pre-feed / queue
                // snapshot can't start playing after it, and no play is credited
                // to the wrong track); an ordinary queue track advances.
                let mode = self.playback_mode();
                if mode.advances_queue_on_finish() {
                    tracing::info!("track finished: advancing queue");
                    self.advance_playback(super::up_next_transport::AdvanceReason::Automatic);
                } else {
                    tracing::info!(?mode, "external media finished: stopping without advancing");
                    self.finish_external();
                }
            }
            PlayerEvent::AdvancedToNext => {
                if self.playback_mode() != super::preview::PlaybackMode::Queue {
                    tracing::warn!("ignoring gapless hand-off during external playback");
                    return;
                }
                // Gapless hand-off: the pre-fed next track is already playing.
                // Advance the queue model and reflect the new track WITHOUT
                // restarting the pipeline. (Real handler wired in below.)
                tracing::info!("gapless hand-off: advancing queue model without restart");
                self.advance_gaplessly();
            }
            PlayerEvent::Spectrum(frame) => {
                let callback = self.song_visual_spectrum_changed.borrow().clone();
                if let Some(callback) = callback {
                    callback(frame);
                }
            }
            PlayerEvent::StreamTags {
                title,
                organization,
            } => {
                tracing::debug!(?title, ?organization, "received stream metadata");
                self.on_stream_tags(title, organization);
            }
            PlayerEvent::Error(message) => {
                // Stage 2 Task 5: this can fire asynchronously for the
                // *currently loaded* queue track (e.g. GStreamer resolving a
                // "file not found"/decode error after `play_track_id`
                // already returned `Ok`) — see `playback_faults.rs`'s doc
                // comment. Only treat it as a per-track failure (diagnose +
                // toast + auto-skip) when there is a current track to
                // attribute it to; otherwise fall back to the pre-Task-5
                // behavior (log + reset) rather than guessing.
                match self.playback_mode() {
                    super::preview::PlaybackMode::Podcast | super::preview::PlaybackMode::Radio => {
                        tracing::error!(%message, "player error during external playback");
                        self.handle_external_error(message);
                        return;
                    }
                    super::preview::PlaybackMode::Preview => {
                        tracing::error!(%message, "player error during preview playback");
                        self.reset_to_stopped();
                        return;
                    }
                    super::preview::PlaybackMode::Queue => {}
                }
                match self.current_track.get() {
                    Some((id, _)) => {
                        tracing::error!(%message, track_id = id, "player error during queue track playback");
                        self.handle_unplayable_track(id);
                    }
                    None => {
                        tracing::error!(%message, "player error with no current track; resetting to stopped");
                        self.reset_to_stopped();
                    }
                }
            }
        }
    }

    /// Stops the pipeline and ensures the bar lands in the stopped/empty
    /// state. Evaluates play tracking for whatever track was loaded first —
    /// every path that ends a listening session (`TrackFinished`, a player
    /// error, a future explicit stop) funnels through here (`play_track_id`
    /// calls it separately for the track-switch case, since that path never
    /// calls `reset_to_stopped`). On success this relies on the `StateChanged
    /// (Stopped)` event `stop()` emits, routed back through `apply_event`, so
    /// the bar isn't reset twice; if `stop()` fails, that event never fires,
    /// so the bar is reset directly here instead. `pub(in crate::ui)` so `mpris_
    /// mirror.rs` and `playback_faults.rs` can call it too.
    pub(in crate::ui) fn reset_to_stopped(&self) {
        self.evaluate_play_tracking();
        // A hard stop leaves every preview/external mode.
        self.leave_external_for_queue();
        self.consecutive_skips.set(0);
        self.failure_skip_limit.set(0);
        // QUE-3: the playback snapshot lives exactly as long as playback —
        // a stop clears it (and its origin), leaving only manual Play Next
        // entries behind; QUE-4's empty state then shows once those are
        // consumed too. Cleared BEFORE `queue_can_resume` below so a fresh
        // stop also disables prev/next when nothing manual is pending.
        {
            let mut queue = self.queue.borrow_mut();
            let repeat = queue.repeat();
            let shuffled = queue.is_shuffled();
            *queue = reprise_core::queue::Queue::new();
            queue.set_repeat(repeat);
            queue.set_shuffle(shuffled);
        }
        self.deferred_queue_purge_id.set(None);
        *self.play_origin.borrow_mut() = None;
        // `now_playing` must be cleared BEFORE the queue notify below: the
        // notify chain synchronously rebuilds a visible Queue view through
        // `queue_view_model`, which reads `now_playing` — clearing after
        // would leave a stale Now Playing section until the next queue
        // event (adversarial review, queue+nav plan, finding 1).
        *self.now_playing.borrow_mut() = None;
        self.notify_queue_changed();
        let queue_can_resume = self.current_up_next.get().is_some()
            || self.queue.borrow().current().is_some()
            || !self.up_next.borrow().is_empty();
        self.sync_transport_enabled(queue_can_resume);
        // Stage 2 Task 6: cleared and mirrored unconditionally, before
        // `player.stop()` even runs — unlike the bar (which relies on the
        // `StateChanged(Stopped)` event on the success path, only resetting
        // directly here on failure), the MPRIS mirror has no such event
        // fallback of its own to lean on, so it's simplest and safest to
        // just always bring it in sync with "stopped, nothing loaded" right
        // here regardless of how `player.stop()` turns out.
        *self.now_playing.borrow_mut() = None;
        self.update_mpris_mirror(MprisPlaybackStatus::Stopped);
        match self.player.stop() {
            Ok(()) => {}
            Err(error) => {
                tracing::error!(%error, "failed to stop player during reset");
                self.sync_state(PlaybackState::Stopped);
                self.sync_clear_track();
            }
        }
    }
}

#[cfg(test)]
mod spectrum_coalescing_tests {
    use super::*;
    use reprise_core::playback::{SpectrumFrame, SPECTRUM_BAND_COUNT};

    fn spectrum(value: f32) -> PlayerEvent {
        PlayerEvent::Spectrum(SpectrumFrame::from_cava_bars([value; SPECTRUM_BAND_COUNT]))
    }

    #[test]
    fn ac_22_spectrum_burst_keeps_only_the_freshest_cava_frame() {
        let events = coalesce_player_event_burst([spectrum(0.25), spectrum(0.5), spectrum(0.75)]);
        assert_eq!(events.len(), 1);
        let PlayerEvent::Spectrum(frame) = &events[0] else {
            panic!("expected the coalesced spectrum");
        };
        assert_eq!(
            frame.bands(),
            SpectrumFrame::from_cava_bars([0.75; SPECTRUM_BAND_COUNT]).bands()
        );
    }

    #[test]
    fn non_spectrum_events_keep_order_and_bound_coalescing_windows() {
        let events = coalesce_player_event_burst([
            spectrum(0.25),
            spectrum(0.5),
            PlayerEvent::StateChanged(PlaybackState::Paused),
            spectrum(0.75),
            spectrum(1.0),
            PlayerEvent::TrackFinished,
        ]);
        assert_eq!(events.len(), 4);
        assert!(matches!(events[0], PlayerEvent::Spectrum(_)));
        assert!(matches!(
            events[1],
            PlayerEvent::StateChanged(PlaybackState::Paused)
        ));
        assert!(matches!(events[2], PlayerEvent::Spectrum(_)));
        assert!(matches!(events[3], PlayerEvent::TrackFinished));
    }
}
