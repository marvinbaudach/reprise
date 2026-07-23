//! Instrumental-preview playback (INST-4b/5b) — the pure playback-mode model
//! plus the `PlayerController` entry points that run a one-off, out-of-queue
//! staging render through the controller's single `Player::play` seam.
//!
//! A *preview* plays an AI instrumental conversion that has **not** been
//! promoted to a library track. It shares the pipeline with ordinary queue
//! playback (there is only one `playbin3`), but it must behave differently in
//! three ways — all captured in the pure [`PlaybackMode`] model below so they
//! are testable without a live pipeline, and applied by [`PlayerController::
//! play_preview`]:
//!
//! 1. **Parked pre-feed.** Entering a preview clears the gapless pre-feed
//!    (`set_next(None)`), so the backend can't hand off from the preview into an
//!    unrelated queue track when the render ends (the stale-gapless bug FIX-1
//!    closes).
//! 2. **No queue advance on finish.** A preview that finishes stops; it never
//!    advances the queue, so a stale queue snapshot can't start playing after a
//!    preview and no play-tracking is credited to the wrong track.
//! 3. **No play credit.** `current_track` stays `None` for the duration of a
//!    preview, so `evaluate_play_tracking` never records a play/scrobble/listen
//!    for it.
//!
//! The bar / Now Playing / MPRIS reflect an explicit, marked preview state (a
//! "(Instrumental preview)" title, the render's own source metadata — never the
//! stale foreign metadata of whatever queue track was loaded before), so media
//! keys and pause act on the preview coherently.

use reprise_core::media_integration::MprisPlaybackStatus;
use reprise_core::playback::{PlaybackError, PlaybackState};

use crate::ui::player_controller::{NowPlaying, PlayerController};
use crate::ui::strings;

/// Whether the controller is playing an ordinary queue track or a one-off
/// instrumental preview. `Queue` is the default (ordinary playback).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(in crate::ui) enum PlaybackMode {
    #[default]
    Queue,
    Preview,
}

impl PlaybackMode {
    /// Whether a `TrackFinished` event in this mode should advance the queue.
    /// Only a queue track does; a finished preview stops instead (INST-4b/5b),
    /// so a stale gapless pre-feed or queue snapshot can never start playing
    /// after a preview — and no play is ever credited to the wrong track.
    pub(in crate::ui) fn advances_queue_on_finish(self) -> bool {
        matches!(self, PlaybackMode::Queue)
    }
}

impl PlayerController {
    /// Whether a staging-render preview is currently playing. Read by
    /// `player_event_handling`'s `TrackFinished` arm to decide advance-vs-stop.
    pub(in crate::ui) fn is_previewing(&self) -> bool {
        self.preview_path.borrow().is_some()
    }

    /// The staging path of the render currently previewing, if any. Lets the
    /// conversion wiring correlate a discard against the live preview (FIX-6).
    pub(in crate::ui) fn previewing_path(&self) -> Option<String> {
        self.preview_path.borrow().clone()
    }

    /// Stops the active preview, if one is playing, WITHOUT advancing or
    /// destroying the queue (same minimal semantics as a natural preview end).
    /// Idempotent: a no-op when nothing is previewing. Called when the render
    /// being previewed is discarded out from under it (FIX-6), so the pipeline
    /// never keeps playing audio from a file that is about to be deleted.
    pub(in crate::ui) fn stop_preview(&self) {
        if self.is_previewing() {
            self.end_preview();
        }
    }

    /// Plays a staging render as a one-off PREVIEW, outside the queue (INST-4b),
    /// through the controller's single `Player::play` seam rather than a raw
    /// backend call. Ends and credits any prior listening session first, then
    /// leaves `current_track` `None` so the preview itself is never credited a
    /// play; parks the gapless pre-feed; enters [`PlaybackMode::Preview`] so a
    /// finish stops instead of advancing the queue; and reflects a marked
    /// "(Instrumental preview)" state across the bar / Now Playing / MPRIS with
    /// no stale foreign metadata. Returns the backend play error (if any) so the
    /// caller can toast.
    pub(in crate::ui) fn play_preview(
        &self,
        path: &str,
        source_title: &str,
        source_artist: &str,
    ) -> Result<(), PlaybackError> {
        // Close (and credit, if earned) the PREVIOUS session while its
        // `now_playing` snapshot is still current — same order as `present_
        // track`.
        self.evaluate_play_tracking();
        self.sync_lyrics_track(None);
        // A preview earns no play credit: leave no creditable current track.
        self.current_track.set(None);
        self.max_position_ms.set(0);
        // Park the gapless pre-feed so the pipeline can't hand off from the
        // preview into an unrelated queue track.
        self.player.set_next(None);
        // Enter preview mode (single source of truth for the mode + FIX-6's
        // discard correlation), in its own statement so no borrow is held
        // across the calls below.
        *self.preview_path.borrow_mut() = Some(path.to_string());

        let title = strings::instrumental_preview_title(source_title);
        *self.now_playing.borrow_mut() = Some(NowPlaying {
            id: 0,
            title: title.clone(),
            artist: source_artist.to_string(),
            album: String::new(),
            album_artist: String::new(),
            genre: String::new(),
            artist_mbid: None,
            art_url: None,
            duration_ms: 0,
            path: path.to_string(),
        });
        self.sync_track(&title, source_artist, "", None);
        // Clears the prior track's cover accent; a render carries no cover so the
        // bar simply shows none (never a stale album's art).
        self.sync_cover(path);

        match self.player.play(path) {
            Ok(()) => {
                self.update_mpris_mirror(MprisPlaybackStatus::Playing);
                Ok(())
            }
            Err(error) => {
                // The pipeline never started: drop back to queue mode so a later
                // `TrackFinished`/stop behaves normally.
                *self.preview_path.borrow_mut() = None;
                Err(error)
            }
        }
    }

    /// The end-of-preview transition: leave preview mode, stop the pipeline, and
    /// mirror the stopped state. Relies on the `StateChanged(Stopped)` event the
    /// `stop()` emits to clear the bar/`now_playing` on the success path (like
    /// `reset_to_stopped`), resetting directly only if `stop()` fails. Deliberately
    /// does NOT clear the queue snapshot — unlike `reset_to_stopped` — so a
    /// preview never discards the queue the user was listening to.
    pub(in crate::ui) fn end_preview(&self) {
        *self.preview_path.borrow_mut() = None;
        // `current_track` is already `None` for a preview, so this credits
        // nothing; kept for symmetry with every other session-ending path.
        self.evaluate_play_tracking();
        self.update_mpris_mirror(MprisPlaybackStatus::Stopped);
        match self.player.stop() {
            Ok(()) => {}
            Err(error) => {
                tracing::error!(%error, "failed to stop after instrumental preview");
                self.sync_state(PlaybackState::Stopped);
                self.sync_clear_track();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PlaybackMode;

    // A finished PREVIEW never advances the queue (so a stale gapless pre-feed /
    // queue snapshot can't start playing after it), while an ordinary queue
    // track does — the load-bearing INST-4b/5b state-machine transition.
    #[test]
    fn preview_mode_finish_stops_while_queue_advances() {
        assert!(
            PlaybackMode::Queue.advances_queue_on_finish(),
            "an ordinary queue track advances the queue when it finishes"
        );
        assert!(
            !PlaybackMode::Preview.advances_queue_on_finish(),
            "a finished preview stops instead of advancing the queue (INST-4b/5b)"
        );
    }

    #[test]
    fn playback_mode_defaults_to_queue() {
        assert_eq!(PlaybackMode::default(), PlaybackMode::Queue);
    }
}
