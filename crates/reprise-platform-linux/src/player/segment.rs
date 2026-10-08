//! Playing a track that a CUE sheet cuts from a larger file.
//!
//! A CUE track is the stretch `[start_ms, end_ms)` of its file. The player
//! keeps that stretch as the active [`Cut`] and everything that faces the
//! frontend is relative to it: the position ticker reports `position − start`
//! against the cut's own length, and a seek lands at `start + position`.
//!
//! The end of a cut is enforced by a buffer probe on the gain element's sink
//! pad, behind the filter's playback queue. That is where the gain changes,
//! but not where a buffer is heard: `playbin`'s sink queue and the audio sink's
//! own buffer still lie downstream, and together they hold about a second.
//! The first buffer whose stream time reaches the boundary either carries on
//! into the armed contiguous successor — the next track of the same file,
//! starting exactly where this one ends — with that track's gain, or, with
//! nothing armed, is dropped and an end-of-stream goes in its place: the audio
//! sink has not played what it was already handed, and only an end-of-stream
//! drains it. The pad refuses everything after that, the file's own
//! end-of-stream included, so the bus sees exactly one end-of-stream and it is
//! the track's `TrackFinished` — which the frontend answers with the next
//! `play()`, so it must not arrive before the tail has been heard.
//!
//! For the same reason a carry-on is only staged at the probe and announced
//! once the sink renders the boundary (see [`handoff`]): until then the
//! frontend keeps the outgoing track, its time and its length.
//!
//! A CUE track starts without waiting: [`start_segment`] sets the file's
//! pipeline to paused and returns, and the bus's `ASYNC_DONE` — the preroll —
//! calls [`SegmentGate::complete_start`], which learns the file's duration and
//! seeks to a nonzero track start. A track beginning with its file needs no
//! seek: flushing `flacparse` back to zero during its fresh preroll can fail the
//! stream. A nonzero seek's own `ASYNC_DONE` then enters Playing, so playback
//! does not overlap its flush either. `play` runs on the GTK main thread, and a
//! file on a slow mount must not freeze it.
//!
//! The last track of a file has no boundary when its end lies within
//! [`OPEN_END_TOLERANCE_MS`] of the file's duration: it plays to the end of the
//! file and finishes like a whole file does. The end a sheet gives its last
//! track is only the probed metadata duration, and cutting the file's real
//! tail off because of it would drop music.
//!
//! The cut, the armed successor and the hand-off share one mutex with the
//! position ticker, which computes and sends every tick under it. The hand-off
//! swaps the cut and sends `AdvancedToNext` under the same lock, so no tick
//! computed against the old cut can follow the `AdvancedToNext` that ends it.

mod handoff;

use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use reprise_core::playback::{PlaybackError, PlaybackState, PlayerEvent};

use crate::gapless::QueuedTrack;
use crate::player_effects::{linear_gain, TRACK_GAIN_NAME};
use handoff::{watch_render, PendingHandOff};

/// An end this close to the file's duration means "to the end of the file".
pub(crate) const OPEN_END_TOLERANCE_MS: i64 = 1000;

const NANOS_PER_MILLI: u64 = 1_000_000;

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

    fn matches(&self, segment: (i64, i64)) -> bool {
        (self.start_ms, self.end_ms) == segment
    }

    fn start_ns(&self) -> u64 {
        self.start_ms.max(0) as u64 * NANOS_PER_MILLI
    }

    fn boundary_ns(&self) -> Option<u64> {
        (!self.open_end).then(|| self.end_ms.max(0) as u64 * NANOS_PER_MILLI)
    }
}

/// What a seek within a CUE track comes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CutSeek {
    /// Seek the pipeline to this file position now.
    Now(i64),
    /// The track has not started yet; its start-seek will go there.
    Deferred,
}

#[derive(Clone, Copy, Debug)]
struct ArmedNext {
    cut: Cut,
    gain_db: f64,
}

#[derive(Debug, Default)]
struct CutState {
    uri: String,
    active: Option<Cut>,
    file_duration_ms: Option<i64>,
    armed: Option<ArmedNext>,
    /// The file position the track's start-seek goes to once the file has
    /// prerolled; `None` once it has landed. A seek before then retargets it.
    pending_start_ms: Option<i64>,
    /// The target of the start-seek whose `ASYNC_DONE` is still pending. Kept
    /// separately from `pending_start_ms`, because an early user seek may
    /// retarget the desired start while this seek is in flight.
    start_seek_ms: Option<i64>,
    /// The probe handed over to the armed successor and the frontend has not
    /// fed a next track since: a re-feed of that track is the one playing.
    handed_off: bool,
    /// The hand-off the stream has made and the sink not yet rendered; the
    /// stream is in its cut while `active` stays the outgoing track's.
    pending: Option<PendingHandOff>,
    /// The linear gain the probe puts back with its next buffer, after a
    /// seek withdrew a staged hand-off.
    restore_gain: Option<f64>,
    /// A flushing seek was asked for and its segment has not reached the
    /// probe yet. Buffers until then predate the seek, and a hand-off they
    /// stage must not be announced: the next post-seek buffer withdraws it.
    seek_in_flight: bool,
}

impl CutState {
    /// The cut the stream at the probe is in: a staged successor's, or the
    /// active one.
    fn streaming(&self) -> Option<Cut> {
        self.pending.map(|pending| pending.next.cut).or(self.active)
    }
}

/// The shared cut state, the event sink ticks and boundaries are sent
/// through, and the stream generation a hand-off bumps.
pub(crate) struct SegmentGate {
    state: Mutex<CutState>,
    on_event: Arc<dyn Fn(PlayerEvent) + Send + Sync>,
    stream_generation: Arc<AtomicU64>,
    /// Counts staged hand-offs, so a watcher never announces a later one.
    handoff_epoch: AtomicU64,
}

pub(crate) type SegmentHandle = Arc<SegmentGate>;

impl SegmentGate {
    pub(crate) fn new(
        on_event: Arc<dyn Fn(PlayerEvent) + Send + Sync>,
        stream_generation: Arc<AtomicU64>,
    ) -> SegmentHandle {
        Arc::new(Self {
            state: Mutex::new(CutState::default()),
            on_event,
            stream_generation,
            handoff_epoch: AtomicU64::new(0),
        })
    }

    fn lock(&self) -> MutexGuard<'_, CutState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Forgets the cut, the armed successor and every flag. Every hard
    /// restart calls it: a cut is only valid for
    /// the stream it was set on.
    pub(crate) fn reset(&self) {
        *self.lock() = CutState::default();
    }

    /// Makes `segment` of `uri` the active cut. `file_duration_ms` is the
    /// pipeline's answer after preroll, cached so neither the probe nor
    /// `route_next` ever has to query the pipeline.
    pub(crate) fn begin(&self, uri: &str, segment: (i64, i64), file_duration_ms: Option<i64>) {
        *self.lock() = CutState {
            uri: uri.to_owned(),
            active: Some(Cut::new(segment.0, segment.1, file_duration_ms)),
            file_duration_ms,
            ..CutState::default()
        };
    }

    /// Makes `segment` of `uri` the active cut before the file has prerolled
    /// and records that its start-seek is still to come. The duration is not
    /// known yet: [`Self::complete_start`] adds it. The cut is installed now
    /// because the frontend feeds the next track right after `play`.
    fn begin_start(&self, uri: &str, segment: (i64, i64)) {
        self.begin(uri, segment, None);
        self.lock().pending_start_ms = Some(segment.0);
    }

    /// Learns the file's duration once it is known: an end within the
    /// tolerance of it opens the track, for the active cut and an armed
    /// successor alike. An open-ended track has no boundary to hand over at.
    fn learn_file_duration(&self, file_duration_ms: Option<i64>) {
        let mut state = self.lock();
        state.file_duration_ms = file_duration_ms;
        state.active = state
            .active
            .map(|cut| Cut::new(cut.start_ms, cut.end_ms, file_duration_ms));
        if state.active.is_some_and(|cut| cut.open_end) {
            state.armed = None;
        }
        state.armed = state.armed.map(|next| ArmedNext {
            cut: Cut::new(next.cut.start_ms, next.cut.end_ms, file_duration_ms),
            ..next
        });
    }

    /// Each start `ASYNC_DONE` of a CUE track's file calls. The preroll one
    /// issues a flushing, sample-accurate seek to a nonzero track start and
    /// leaves the pipeline Paused. A zero start enters Playing directly: the
    /// parser is already there, and a redundant flush back to zero is the
    /// failure this path must avoid. A nonzero seek's own `ASYNC_DONE` enters
    /// Playing, so the parser never has to stream while that flush is still in
    /// flight. An early user seek that changed the desired start issues its
    /// replacement here first. A refused seek is logged and the track plays
    /// from where the file stands, because failing would mark a playable file
    /// missing.
    ///
    /// Does nothing unless a start is pending and `playbin` has really
    /// prerolled: a message left over from a pipeline that has since been
    /// restarted arrives while the new one still prerolls.
    pub(crate) fn complete_start(&self, playbin: &gst::Element) {
        let (prerolled, _, _) = playbin.state(gst::ClockTime::ZERO);
        if matches!(prerolled, Ok(gst::StateChangeSuccess::Async) | Err(_)) {
            return;
        }
        let (next_seek_ms, learn_duration) = {
            let mut state = self.lock();
            let Some(pending_start_ms) = state.pending_start_ms else {
                return;
            };
            let learn_duration = state.start_seek_ms.is_none();
            if state.start_seek_ms == Some(pending_start_ms)
                || (pending_start_ms <= 0 && state.start_seek_ms.is_none())
            {
                state.pending_start_ms = None;
                state.start_seek_ms = None;
                (None, learn_duration)
            } else {
                state.start_seek_ms = Some(pending_start_ms);
                (Some(pending_start_ms), learn_duration)
            }
        };
        if learn_duration {
            let file_duration_ms = playbin
                .query_duration::<gst::ClockTime>()
                .map(|duration| duration.mseconds() as i64);
            self.learn_file_duration(file_duration_ms);
        }
        if let Some(start_ms) = next_seek_ms {
            let start = gst::ClockTime::from_mseconds(start_ms.max(0) as u64);
            match playbin.seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE, start) {
                Ok(()) => return,
                Err(error) => {
                    let mut state = self.lock();
                    state.pending_start_ms = None;
                    state.start_seek_ms = None;
                    tracing::warn!(%error, start_ms, "could not seek to the CUE track's start");
                }
            }
        }
        // Before `Playing`, so no event of the new stream carries the old one.
        self.stream_generation.fetch_add(1, Ordering::SeqCst);
        if let Err(error) = playbin.set_state(gst::State::Playing) {
            // GStreamer posts the failure on the bus as well.
            tracing::warn!(%error, "CUE track's file would not start playing");
            return;
        }
        (self.on_event)(PlayerEvent::StateChanged(PlaybackState::Playing));
    }

    /// Routes the next track the frontend feeds and returns whether it may
    /// go into the whole-file URI slot — only when neither it nor the playing
    /// track is a CUE track. The next track of the same file, starting where
    /// the active cut ends (which therefore has a boundary), is armed for the
    /// probe instead; anything else disarms. Last write wins.
    ///
    /// A hand-off staged but not yet heard is withdrawn: the frontend still
    /// plays the outgoing track and has just named a different successor for
    /// it (a re-feed of the staged one never gets here, see
    /// `Player::refresh_in_flight_gain`). With nothing armed the probe ends the
    /// stream with its next buffer. What already lies downstream of the probe
    /// cannot be recalled: up to a second of the withdrawn successor is still
    /// heard, under the outgoing track's title and with its clock clamped at
    /// its end, before `TrackFinished` lets the newly named track start
    /// (PLAY-23a).
    pub(crate) fn route_next(&self, next: Option<&QueuedTrack>) -> bool {
        let mut state = self.lock();
        state.handed_off = false;
        state.pending = None;
        let armed = match (state.active, next) {
            (Some(active), Some(next)) => next.segment.and_then(|(start_ms, end_ms)| {
                let contiguous =
                    state.uri == next.uri && !active.open_end && active.end_ms == start_ms;
                contiguous.then(|| ArmedNext {
                    cut: Cut::new(start_ms, end_ms, state.file_duration_ms),
                    gain_db: next.gain_db,
                })
            }),
            _ => None,
        };
        state.armed = armed;
        state.active.is_none() && next.is_some_and(|next| next.segment.is_none())
    }

    /// Whether a CUE track is playing.
    pub(crate) fn cue_active(&self) -> bool {
        self.lock().active.is_some()
    }

    /// Whether `segment` of `uri` is the track a hand-off is staged for or has
    /// already gone to, the frontend not having moved on since. If so, its gain
    /// becomes `gain_db` and the caller applies it to the pipeline.
    pub(crate) fn refresh_in_flight_gain(
        &self,
        uri: &str,
        segment: (i64, i64),
        gain_db: f64,
    ) -> bool {
        let mut state = self.lock();
        if state.uri != uri {
            return false;
        }
        if let Some(pending) = state.pending.as_mut() {
            if pending.next.cut.matches(segment) {
                pending.next.gain_db = gain_db;
                return true;
            }
            return false;
        }
        state.handed_off && state.active.is_some_and(|active| active.matches(segment))
    }

    /// Where a seek to `position_ms` of the active CUE track goes, or `None`
    /// for a whole file. Before the file has prerolled there is nothing to
    /// seek in: the seek only retargets the start-seek that is still to come.
    /// Otherwise the flush it sends clears the end-of-stream the boundary
    /// pushed, so the boundary fires again when playback reaches it.
    ///
    /// The seek is in the track the frontend shows, so a hand-off staged but
    /// not yet heard is withdrawn before the watcher can announce it.
    pub(crate) fn seek_target_ms(&self, position_ms: i64) -> Option<CutSeek> {
        let mut state = self.lock();
        let cut = state.active?;
        Self::withdraw_handoff(&mut state);
        let target_ms = cut.seek_target_ms(position_ms, state.file_duration_ms);
        if state.pending_start_ms.is_some() {
            state.pending_start_ms = Some(target_ms);
            return Some(CutSeek::Deferred);
        }
        state.seek_in_flight = true;
        Some(CutSeek::Now(target_ms))
    }

    /// A seek [`Self::seek_target_ms`] asked for was refused: no flush will
    /// come to end the wait it started.
    pub(crate) fn seek_refused(&self) {
        self.lock().seek_in_flight = false;
    }

    /// The flushing seek's new segment reached the probe: buffers from here
    /// on are the seek's own.
    fn seek_landed(&self) {
        self.lock().seek_in_flight = false;
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

/// Makes `segment` of `uri` the active cut and sets `playbin` to paused,
/// without waiting for the file: the preroll's `ASYNC_DONE` completes the
/// start (see [`SegmentGate::complete_start`]). Runs under
/// `Player::try_play`'s `playbin` lock, after the URI is set. A file that
/// cannot even be set paused fails the attempt; one that cannot be read fails
/// on the bus, as a whole file does.
pub(super) fn start_segment(
    playbin: &gst::Element,
    gate: &SegmentGate,
    uri: &str,
    segment: (i64, i64),
) -> Result<(), PlaybackError> {
    gate.begin_start(uri, segment);
    playbin
        .set_state(gst::State::Paused)
        .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))?;
    Ok(())
}

/// Installs the boundary probe on the gain element's sink pad of `playbin`'s
/// filter. Every buffer passes untouched while no cut is active, which is why
/// the crossfade secondary carries it too.
pub(crate) fn install_segment_boundary(
    playbin: &gst::Element,
    gate: SegmentHandle,
) -> Result<(), PlaybackError> {
    let gain = playbin
        .property::<Option<gst::Element>>("audio-filter")
        .and_then(|filter| filter.downcast::<gst::Bin>().ok())
        .and_then(|bin| bin.by_name(TRACK_GAIN_NAME))
        .ok_or_else(|| PlaybackError::Backend("GStreamer: playbin has no track gain".into()))?;
    let sink = gain
        .static_pad("sink")
        .ok_or_else(|| PlaybackError::Backend("GStreamer: track gain has no sink pad".into()))?;
    // Weak: the probe lives inside the pipeline it watches.
    let watched = playbin.downgrade();
    // The segment belongs to this pad's stream, not to the shared gate: the
    // crossfade secondary carries the same gate and must not overwrite it.
    let stream_segment = Mutex::new(None::<gst::FormattedSegment<gst::ClockTime>>);
    sink.add_probe(
        gst::PadProbeType::BUFFER | gst::PadProbeType::EVENT_DOWNSTREAM,
        move |pad, info| match &info.data {
            Some(gst::PadProbeData::Event(event)) => {
                if let gst::EventView::Segment(segment) = event.view() {
                    gate.seek_landed();
                    *stream_segment
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner) =
                        segment.segment().downcast_ref::<gst::ClockTime>().cloned();
                }
                gst::PadProbeReturn::Ok
            }
            Some(gst::PadProbeData::Buffer(buffer)) => {
                let stream_time = buffer.pts().and_then(|pts| {
                    match stream_segment
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .as_ref()
                    {
                        Some(segment) => segment.to_stream_time(pts),
                        None => Some(pts),
                    }
                });
                match gate.on_buffer(stream_time, &gain) {
                    BoundaryVerdict::Pass => gst::PadProbeReturn::Ok,
                    BoundaryVerdict::HandOff(epoch) => {
                        watch_render(&gate, watched.clone(), epoch);
                        gst::PadProbeReturn::Ok
                    }
                    BoundaryVerdict::EndOfTrack => {
                        // Outside the cut lock: the event blocks this
                        // streaming thread until the sink has drained.
                        pad.send_event(gst::event::Eos::new());
                        gst::PadProbeReturn::Drop
                    }
                }
            }
            _ => gst::PadProbeReturn::Ok,
        },
    );
    Ok(())
}

/// What the probe does with a buffer.
enum BoundaryVerdict {
    Pass,
    /// The next track starts with this buffer: pass it and watch for the
    /// staged hand-off with this epoch to be heard.
    HandOff(u64),
    /// The track ends before this buffer: drop it and end the stream.
    EndOfTrack,
}

impl SegmentGate {
    /// The probe's decision for one buffer about to reach the gain element —
    /// see the module comment. Runs on the streaming thread; it takes only the
    /// cut lock, never the `playbin` lock `try_play` holds across `Null`.
    fn on_buffer(
        &self,
        stream_time: Option<gst::ClockTime>,
        gain: &gst::Element,
    ) -> BoundaryVerdict {
        let mut state = self.lock();
        if let Some(outgoing_gain) = state.restore_gain.take() {
            gain.set_property("volume", outgoing_gain);
        }
        let Some(stream_time) = stream_time else {
            return BoundaryVerdict::Pass;
        };
        // A buffer before the staged successor's start: a flushing seek went
        // back into the outgoing track, which is playing on.
        if state
            .pending
            .is_some_and(|pending| stream_time.nseconds() < pending.next.cut.start_ns())
        {
            Self::withdraw_handoff(&mut state);
            if let Some(outgoing_gain) = state.restore_gain.take() {
                gain.set_property("volume", outgoing_gain);
            }
        }
        let Some(cut) = state.streaming() else {
            return BoundaryVerdict::Pass;
        };
        let Some(boundary_ns) = cut.boundary_ns() else {
            return BoundaryVerdict::Pass;
        };
        if stream_time.nseconds() < boundary_ns {
            return BoundaryVerdict::Pass;
        }
        match state.armed.take() {
            Some(next) => {
                let outgoing_gain = gain.property::<f64>("volume");
                gain.set_property("volume", linear_gain(next.gain_db));
                tracing::debug!(boundary_ms = cut.end_ms, "cue: contiguous hand-off staged");
                BoundaryVerdict::HandOff(self.stage_handoff(&mut state, next, outgoing_gain))
            }
            None => {
                tracing::debug!(boundary_ms = cut.end_ms, "cue: track reached its end");
                BoundaryVerdict::EndOfTrack
            }
        }
    }
}
