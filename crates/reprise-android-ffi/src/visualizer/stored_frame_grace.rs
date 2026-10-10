//! Keeps a swipe's adopted shape on screen while the new stream's first PCM is
//! still buffering, and remembers the stored frame that was held back.

use std::time::Duration;

use reprise_core::visuals::spectrum_frame_from_bands;

#[cfg(test)]
use super::AndroidVisualEngine;
use super::VisualState;

/// How long a stored-analysis frame may not replace an adopted shape.
///
/// The first PCM of a flipped-to stream arrives about 200 ms after
/// `adopt_shape` on the device (2026-10-09 acceptance run); 500 ms leaves room
/// for slower buffering. A device that never delivers PCM falls back to stored
/// frames this much later than it used to. Deliberately its own literal rather
/// than an alias of `LIVE_AUDIO_STALE_AFTER`: the two bound different things
/// and must be free to change independently.
pub(crate) const ADOPTED_SHAPE_STORED_FRAME_GRACE: Duration = Duration::from_millis(500);

/// The deadline until which stored frames are held back, plus the latest frame
/// that was held back.
///
/// It is independent of `AdoptedShapeHold`'s phase because a stream reset
/// clears that phase, and a reset between the adoption and the first PCM must
/// not let stored frames through.
#[derive(Default)]
pub(crate) struct StoredFrameGrace {
    until: Option<Duration>,
    remembered: Option<Vec<f32>>,
    // Whether the deadline guards a fresh start rather than an adopted shape:
    // the engine then also stays on its resting projection, and a pause ends
    // the grace.
    fresh_start: bool,
}

impl StoredFrameGrace {
    /// Arms the deadline for a fresh adoption and forgets any earlier frame.
    pub(crate) fn begin(&mut self, now: Duration) {
        self.until = Some(now.saturating_add(ADOPTED_SHAPE_STORED_FRAME_GRACE));
        self.remembered = None;
        self.fresh_start = false;
    }

    /// Arms the deadline for a fresh start: stored frames wait for the first
    /// PCM, and the engine keeps its resting projection until then.
    pub(crate) fn begin_fresh_start(&mut self, now: Duration) {
        self.begin(now);
        self.fresh_start = true;
    }

    /// Ends the grace and forgets the remembered frame.
    pub(crate) fn clear(&mut self) {
        self.until = None;
        self.remembered = None;
        self.fresh_start = false;
    }

    /// Ends a fresh start's grace, and only that: a swipe's grace survives the
    /// pause blip of an item change.
    pub(crate) fn end_fresh_start(&mut self) {
        if self.fresh_start {
            self.clear();
        }
    }

    /// Whether the deadline of a fresh start's grace is still ahead.
    pub(crate) fn holds_fresh_start(&self, now: Duration) -> bool {
        self.fresh_start && self.until.is_some_and(|deadline| now < deadline)
    }

    /// Whether the grace is running, whatever it guards.
    pub(crate) fn is_running(&self, now: Duration) -> bool {
        self.until.is_some_and(|deadline| now < deadline)
    }

    /// Holds `bands` back and remembers them (latest only) while the grace
    /// runs; returns whether the caller must leave the display alone.
    pub(crate) fn hold(&mut self, bands: &[f32], now: Duration) -> bool {
        // A track with no stored analysis has no frame to flash: its resting
        // scene shows at once instead of the cover waiting out the grace.
        if self.fresh_start && bands.is_empty() {
            return false;
        }
        if self.until.is_some_and(|deadline| now < deadline) {
            self.remembered = Some(bands.to_vec());
            return true;
        }
        // Expired: the caller ingests this frame, which supersedes anything
        // remembered earlier.
        self.clear();
        false
    }

    /// The remembered frame, once, if the grace has expired by `now`.
    pub(crate) fn take_expired(&mut self, now: Duration) -> Option<Vec<f32>> {
        match self.until {
            Some(deadline) if now >= deadline => {
                self.until = None;
                self.remembered.take()
            }
            _ => None,
        }
    }
}

impl VisualState {
    /// Whether anything gives the engine a picture to play: stored analysis
    /// (not while a fresh start still waits for the first PCM), an adopted
    /// shape, live audio, or a reset that holds the display.
    pub(super) fn has_audio(&self, now: Duration) -> bool {
        if self.stored_frame_grace.holds_fresh_start(now) {
            return false;
        }
        self.has_analysis
            || self.adopted_shape_hold.is_active()
            || self.has_live_audio
            || self.awaiting_stream_after_reset
    }

    /// Called on the edge into playing. A stream with no audio of its own yet
    /// and no adopted shape to continue would otherwise draw the stored frame
    /// at its own normalisation for the first-PCM delay, then drop to the live
    /// level (#1181). Waits like a swipe's adopted shape does, bounded by the
    /// same grace.
    pub(super) fn begin_fresh_start_hold(&mut self, now: Duration) {
        if self.has_live_audio
            || self.adopted_shape_hold.is_active()
            || self.stored_frame_grace.is_running(now)
        {
            return;
        }
        self.stored_frame_grace.begin_fresh_start(now);
        self.set_engine_playing(self.playing && self.has_audio(now), now);
    }

    /// Hands the screen back to the stored analysis once a fresh start's grace
    /// has run out with no PCM and no frame left to ingest. Returns whether
    /// the engine started playing.
    pub(super) fn release_expired_fresh_start_hold(&mut self, now: Duration) -> bool {
        if !self.stored_frame_grace.fresh_start || self.stored_frame_grace.is_running(now) {
            return false;
        }
        self.stored_frame_grace.clear();
        let was_playing = self.engine_playing;
        self.set_engine_playing(self.playing && self.has_audio(now), now);
        self.engine_playing != was_playing
    }

    /// Installs one stored-analysis frame as the displayed shape.
    pub(super) fn ingest_stored_frame(&mut self, bands: &[f32], now: Duration) {
        let has_analysis = !bands.is_empty();
        let frame = spectrum_frame_from_bands(bands);
        self.engine.set_retain_paused_live_shape(false);
        self.engine.set_has_track(true);
        let playing = self.playing;
        self.set_engine_playing(playing && has_analysis, now);
        self.engine.ingest(&frame);
        self.has_ingested = true;
        self.has_analysis = has_analysis;
        // The new stream has spoken, even when all it said was "nothing": an
        // empty frame must not leave the post-reset hold pinning the old picture.
        self.awaiting_stream_after_reset = false;
        if has_analysis {
            self.last_live_bands = None;
        }
    }

    /// Ingests the frame the grace held back, if it has expired and no live
    /// PCM has taken over. `SceneDriver` resends a stored frame only when the
    /// playhead moves, so a stalled playhead would otherwise keep the adopted
    /// shape past the expiry. Returns whether a frame was ingested.
    pub(super) fn ingest_remembered_stored_frame(&mut self, now: Duration) -> bool {
        if self.has_live_audio {
            return false;
        }
        let Some(bands) = self.stored_frame_grace.take_expired(now) else {
            return false;
        };
        self.ingest_stored_frame(&bands, now);
        self.stored_frame_grace.clear();
        true
    }
}

#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct IngestFlags {
    pub(crate) awaiting_stream_after_reset: bool,
    pub(crate) has_analysis: bool,
}

#[cfg(test)]
impl AndroidVisualEngine {
    pub(crate) fn ingest_flags_for_testing(&self) -> IngestFlags {
        let state = self.lock();
        IngestFlags {
            awaiting_stream_after_reset: state.awaiting_stream_after_reset,
            has_analysis: state.has_analysis,
        }
    }

    pub(crate) fn remembers_stored_frame_for_testing(&self) -> bool {
        self.lock().stored_frame_grace.remembered.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LATER: Duration = Duration::from_millis(600);

    #[test]
    fn a_held_frame_is_remembered_and_only_the_latest_survives() {
        let mut grace = StoredFrameGrace::default();
        grace.begin(Duration::ZERO);

        assert!(grace.hold(&[0.1], Duration::from_millis(10)));
        assert!(grace.hold(&[0.2], Duration::from_millis(20)));

        assert_eq!(grace.take_expired(LATER), Some(vec![0.2]));
        assert_eq!(grace.take_expired(LATER), None, "taken only once");
    }

    #[test]
    fn nothing_is_taken_before_the_deadline() {
        let mut grace = StoredFrameGrace::default();
        grace.begin(Duration::ZERO);
        grace.hold(&[0.1], Duration::from_millis(10));

        assert_eq!(grace.take_expired(Duration::from_millis(499)), None);
        assert_eq!(
            grace.take_expired(Duration::from_millis(500)),
            Some(vec![0.1])
        );
    }

    #[test]
    fn clearing_and_rearming_forget_the_remembered_frame() {
        let mut cleared = StoredFrameGrace::default();
        cleared.begin(Duration::ZERO);
        cleared.hold(&[0.1], Duration::from_millis(10));
        cleared.clear();
        assert_eq!(cleared.take_expired(LATER), None);

        let mut rearmed = StoredFrameGrace::default();
        rearmed.begin(Duration::ZERO);
        rearmed.hold(&[0.1], Duration::from_millis(10));
        rearmed.begin(Duration::from_millis(20));
        assert_eq!(rearmed.take_expired(LATER), None);
    }

    #[test]
    fn a_frame_arriving_after_the_deadline_is_not_held() {
        let mut grace = StoredFrameGrace::default();
        grace.begin(Duration::ZERO);
        grace.hold(&[0.1], Duration::from_millis(10));

        assert!(!grace.hold(&[0.2], LATER));
        assert_eq!(grace.take_expired(LATER), None);
    }
}
