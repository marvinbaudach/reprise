//! Playing a track that a CUE sheet cuts from a larger file.
//!
//! A CUE track is the stretch `[start_ms, end_ms)` of its file. The player
//! keeps that stretch as the active [`Cut`] and everything that faces the
//! frontend is relative to it: the position ticker reports `position − start`
//! against the cut's own length, and a seek lands at `start + position`.
//!
//! The cut shares one mutex with the position ticker, which computes and
//! sends every tick under it, so a tick is always computed against the cut
//! that is current when it is sent.

use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use reprise_core::playback::{PlaybackError, PlayerEvent};

/// An end this close to the file's duration means "to the end of the file".
pub(crate) const OPEN_END_TOLERANCE_MS: i64 = 1000;

/// How long `play` waits for a CUE track's file to preroll before it seeks to
/// the track's start. A local file prerolls in milliseconds; one that has not
/// after this long counts as a failed attempt.
const SEGMENT_PREROLL_TIMEOUT: Duration = Duration::from_secs(5);

/// The stretch of a file one CUE track covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Cut {
    pub(crate) start_ms: i64,
    pub(crate) end_ms: i64,
    /// The track plays to the end of the file; no boundary is enforced.
    pub(crate) open_end: bool,
}

impl Cut {
    /// A cut whose end is open when it lies within the tolerance of a known
    /// file duration. An unknown duration keeps the boundary: cutting a track
    /// at its own end is never wrong, only possibly early.
    pub(crate) fn new(start_ms: i64, end_ms: i64, file_duration_ms: Option<i64>) -> Self {
        let open_end = file_duration_ms
            .is_some_and(|duration_ms| end_ms >= duration_ms - OPEN_END_TOLERANCE_MS);
        Self {
            start_ms,
            end_ms,
            open_end,
        }
    }

    /// The cut's length: to its end, or to the end of the file when open.
    pub(crate) fn length_ms(&self, file_duration_ms: Option<i64>) -> i64 {
        let end_ms = match file_duration_ms {
            Some(duration_ms) if self.open_end && duration_ms > 0 => duration_ms,
            _ => self.end_ms,
        };
        (end_ms - self.start_ms).max(0)
    }

    /// `(position, duration)` relative to the cut, the position clamped into
    /// the cut: right after a seek or a hand-off the sink can still answer a
    /// position a little outside it.
    pub(crate) fn relative(
        &self,
        file_position_ms: i64,
        file_duration_ms: Option<i64>,
    ) -> (i64, i64) {
        let length_ms = self.length_ms(file_duration_ms);
        let position_ms = (file_position_ms - self.start_ms).clamp(0, length_ms);
        (position_ms, length_ms)
    }

    /// The file position a seek to `position_ms` of this track goes to; never
    /// past the track's last millisecond.
    pub(crate) fn seek_target_ms(&self, position_ms: i64, file_duration_ms: Option<i64>) -> i64 {
        let last_ms = (self.length_ms(file_duration_ms) - 1).max(0);
        self.start_ms + position_ms.clamp(0, last_ms)
    }
}

#[derive(Debug, Default)]
struct CutState {
    active: Option<Cut>,
    file_duration_ms: Option<i64>,
}

/// The shared cut state and the event sink ticks are sent through.
pub(crate) struct SegmentGate {
    state: Mutex<CutState>,
    on_event: Arc<dyn Fn(PlayerEvent) + Send + Sync>,
}

pub(crate) type SegmentHandle = Arc<SegmentGate>;

impl SegmentGate {
    pub(crate) fn new(on_event: Arc<dyn Fn(PlayerEvent) + Send + Sync>) -> SegmentHandle {
        Arc::new(Self {
            state: Mutex::new(CutState::default()),
            on_event,
        })
    }

    fn lock(&self) -> MutexGuard<'_, CutState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Forgets the cut. Every hard restart calls it: a cut is only valid for
    /// the stream it was set on.
    pub(crate) fn reset(&self) {
        *self.lock() = CutState::default();
    }

    /// Makes `segment` the active cut. `file_duration_ms` is the
    /// pipeline's answer after preroll, cached for an open end.
    pub(crate) fn begin(&self, segment: (i64, i64), file_duration_ms: Option<i64>) {
        *self.lock() = CutState {
            active: Some(Cut::new(segment.0, segment.1, file_duration_ms)),
            file_duration_ms,
        };
    }

    /// The file position a seek to `position_ms` of the active CUE track goes
    /// to, or `None` for a whole file.
    pub(crate) fn seek_target_ms(&self, position_ms: i64) -> Option<i64> {
        let state = self.lock();
        let cut = state.active?;
        Some(cut.seek_target_ms(position_ms, state.file_duration_ms))
    }

    /// Computes the tick for the active cut — or `whole_file` for a whole
    /// file — and sends it, all under the cut lock (see the module comment).
    /// Returns what was sent.
    pub(crate) fn send_tick(
        &self,
        file_position_ms: i64,
        queried_duration_ms: i64,
        whole_file: impl FnOnce() -> (i64, i64),
    ) -> (i64, i64) {
        let state = self.lock();
        let (position_ms, duration_ms) = match state.active {
            Some(cut) => {
                let file_duration_ms = (queried_duration_ms > 0)
                    .then_some(queried_duration_ms)
                    .or(state.file_duration_ms);
                cut.relative(file_position_ms, file_duration_ms)
            }
            None => whole_file(),
        };
        (self.on_event)(PlayerEvent::Position {
            position_ms,
            duration_ms,
        });
        (position_ms, duration_ms)
    }
}

/// Prerolls a CUE track's file paused, makes `segment` the active cut and
/// seeks — flushing and sample-accurate — to its start, so the first sample
/// heard is the track's own. Runs under `Player::try_play`'s `playbin` lock,
/// after the URI is set and before `Playing`. A file that does not preroll
/// fails the attempt; a refused seek is logged and the track plays from where
/// the file stands, because failing would mark a playable file missing.
pub(super) fn start_segment(
    playbin: &gst::Element,
    gate: &SegmentGate,
    segment: (i64, i64),
) -> Result<(), PlaybackError> {
    playbin
        .set_state(gst::State::Paused)
        .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))?;
    let (prerolled, _, _) = playbin.state(gst::ClockTime::from_mseconds(
        SEGMENT_PREROLL_TIMEOUT.as_millis() as u64,
    ));
    match prerolled {
        Ok(gst::StateChangeSuccess::Async) => {
            return Err(PlaybackError::Backend(
                "GStreamer: CUE track's file did not preroll".into(),
            ))
        }
        Ok(_) => {}
        Err(error) => return Err(PlaybackError::Backend(format!("GStreamer: {error}"))),
    }
    let file_duration_ms = playbin
        .query_duration::<gst::ClockTime>()
        .map(|duration| duration.mseconds() as i64);
    gate.begin(segment, file_duration_ms);
    let start = gst::ClockTime::from_mseconds(segment.0.max(0) as u64);
    if let Err(error) = playbin.seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE, start)
    {
        tracing::warn!(%error, start_ms = segment.0, "could not seek to the CUE track's start");
    }
    Ok(())
}
