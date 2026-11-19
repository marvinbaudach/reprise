//! Stop, pause and seek: each a job on the pipeline's own queue, never a call
//! the main context waits on (see `pipeline_queue.rs`).

use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::PoisonError;

use reprise_core::playback::{PlaybackError, PlaybackState, PlayerEvent};

use super::pipeline_queue::Asked;
use super::segment::CutSeek;
use super::{Player, START_ANSWER_WAIT};

impl Player {
    /// A full stop tears down everything: abort any crossfade (retiring the
    /// incoming pipeline) and clear the gapless slot, then retire the playing
    /// pipeline behind its own work and leave an idle one in the slot. A late
    /// error of the stopped start cannot reach the frontend: the retired
    /// pipeline's bus watch is gone.
    pub(super) fn stop_playback(&self) -> Result<(), PlaybackError> {
        self.reset_transition();
        self.start_watchdog.disarm();
        if let Err(error) = self.swap.replace_with_idle(None) {
            tracing::warn!(%error, "stop could not leave an idle pipeline behind");
        }
        (self.on_event)(PlayerEvent::StateChanged(PlaybackState::Stopped));
        Ok(())
    }

    /// Follows the state the user asked for, not `current_state()`, which lags
    /// behind queued jobs. Submits the change without waiting.
    pub(super) fn toggle_pause_queued(&self) -> Result<PlaybackState, PlaybackError> {
        let next = {
            let playbin = self.playbin.lock().unwrap_or_else(PoisonError::into_inner);
            let (asked, state, reported) = match playbin.asked() {
                Asked::Idle => return Ok(PlaybackState::Stopped),
                Asked::Playing => (Asked::Paused, gst::State::Paused, PlaybackState::Paused),
                Asked::Paused => (Asked::Playing, gst::State::Playing, PlaybackState::Playing),
            };
            playbin.set_asked(asked);
            playbin.submit(move |element| {
                if let Err(error) = element.set_state(state) {
                    tracing::warn!(%error, ?state, "the pause toggle's state change failed");
                }
            });
            reported
        };
        (self.on_event)(PlayerEvent::StateChanged(next));
        Ok(next)
    }

    pub(super) fn seek_queued(&self, position_ms: i64) -> Result<(), PlaybackError> {
        // A seek within the current track abandons any crossfade that may have
        // begun in its tail (the overlap position no longer applies); the
        // gapless slot stays valid — we are still on the same track. `next_uri`
        // is left intact so an already-in-progress fade that consumed it does
        // not silently lose the successor for the rest of the track.
        self.abort_crossfade();
        let (flags, target_ms) = match self.segments.seek_target_ms(position_ms) {
            Some(CutSeek::Deferred) => return Ok(()),
            Some(CutSeek::Now(file_position_ms)) => (
                gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
                file_position_ms,
            ),
            None => (
                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                position_ms,
            ),
        };
        let segments = self.segments.clone();
        let answer = {
            let playbin = self.playbin.lock().unwrap_or_else(PoisonError::into_inner);
            let queue = playbin.queue();
            playbin.ask(move |element| {
                let sought = element.seek_simple(
                    flags,
                    gst::ClockTime::from_mseconds(target_ms.max(0) as u64),
                );
                if sought.is_err() {
                    // The job clears the flag itself, so a late refusal does too
                    // (unless its pipeline has been retired meanwhile).
                    segments.seek_refused(&queue);
                }
                sought
            })
        };
        match answer.wait(START_ANSWER_WAIT) {
            Some(Ok(())) => Ok(()),
            Some(Err(error)) => Err(PlaybackError::Backend(format!("GStreamer: {error}"))),
            // Still queued: it may yet land, and the frontend's one retry of
            // the same target is harmless.
            None => Err(PlaybackError::Backend(
                "GStreamer: the seek was not answered in time".into(),
            )),
        }
    }
}
