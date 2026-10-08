//! Announcing a contiguous CUE hand-off when it is heard, not when it is seen.
//!
//! The boundary probe meets the boundary about a second before the audio sink
//! renders it: everything downstream of the gain element — `playbin`'s own
//! sink queue and the sink's ring buffer — still holds the outgoing track's
//! tail. The probe therefore only *stages* the hand-off: it switches the gain,
//! because the very next buffer is the new track's, and records a
//! [`PendingHandOff`]. The cut the frontend sees stays the outgoing one, so
//! its clock keeps counting to its end.
//!
//! A watcher thread then polls the pipeline's position — the stream time the
//! sink is rendering, its own latency included — and once it reaches the
//! boundary swaps the cut and sends `AdvancedToNext`, under the cut lock like
//! every tick. Anything that invalidates the staged hand-off before then
//! withdraws it: a seek back into the outgoing track re-arms the successor and
//! restores the outgoing gain, a different successor fed meanwhile lets the
//! outgoing track end at its boundary, and a restart forgets it.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};
use std::time::Duration;

use gstreamer as gst;
use gstreamer::glib;
use gstreamer::prelude::*;

use reprise_core::playback::PlayerEvent;

use super::{ArmedNext, CutState, SegmentGate, NANOS_PER_MILLI};

/// How often the watcher reads the position while playing: the hand-off is
/// announced at most this long after it is heard.
const RENDER_POLL_PLAYING: Duration = Duration::from_millis(10);
/// How often it looks while paused or prerolling, when the position stands.
const RENDER_POLL_IDLE: Duration = Duration::from_millis(100);
/// How many polls in a row, while playing, the pipeline may leave the position
/// unanswered before the hand-off is announced unmeasured — about a second.
/// One unanswered query is no reason: a flushing seek can leave the sink
/// without a position for a moment.
const UNANSWERED_POLLS_TOLERATED: u32 = 100;

/// A hand-off the probe has carried out in the stream but the sink has not
/// rendered yet.
#[derive(Clone, Copy, Debug)]
pub(super) struct PendingHandOff {
    pub(super) next: ArmedNext,
    /// The gain the outgoing track played at, for a seek back into it.
    pub(super) outgoing_gain: f64,
    /// Tells this hand-off's watcher from a later one's.
    pub(super) epoch: u64,
}

impl PendingHandOff {
    /// The file position at which the next track is heard.
    pub(super) fn boundary(&self) -> gst::ClockTime {
        gst::ClockTime::from_nseconds(self.next.cut.start_ms.max(0) as u64 * NANOS_PER_MILLI)
    }
}

impl SegmentGate {
    /// Stages the hand-off to `next` at the probe: the gain switches now, the
    /// announcement waits for the render. Returns the epoch the watcher keys on.
    pub(super) fn stage_handoff(
        &self,
        state: &mut CutState,
        next: ArmedNext,
        outgoing_gain: f64,
    ) -> u64 {
        // A staged hand-off still unheard when the next boundary is met has
        // been played through: it is announced first, so none is lost.
        if let Some(earlier) = state.pending.take() {
            self.announce(state, earlier);
        }
        let epoch = self.handoff_epoch.fetch_add(1, Ordering::SeqCst) + 1;
        state.pending = Some(PendingHandOff {
            next,
            outgoing_gain,
            epoch,
        });
        epoch
    }

    /// Withdraws the staged hand-off after a seek back into the outgoing track:
    /// the successor is armed again and the outgoing gain comes back with the
    /// probe's next buffer.
    pub(super) fn withdraw_handoff(state: &mut CutState) {
        if let Some(pending) = state.pending.take() {
            state.armed = Some(pending.next);
            state.restore_gain = Some(pending.outgoing_gain);
        }
    }

    /// The swap the frontend sees: the next track's cut becomes the active
    /// one and `AdvancedToNext` goes out, under the cut lock (see the module
    /// comment of `segment.rs`).
    fn announce(&self, state: &mut CutState, pending: PendingHandOff) {
        state.active = Some(pending.next.cut);
        state.handed_off = true;
        self.stream_generation.fetch_add(1, Ordering::SeqCst);
        tracing::debug!(
            boundary_ms = pending.next.cut.start_ms,
            "cue: hand-off heard"
        );
        (self.on_event)(PlayerEvent::AdvancedToNext);
    }

    /// Announces the staged hand-off `epoch` if it still stands.
    fn complete_handoff(&self, epoch: u64) {
        let mut state = self.lock();
        if let Some(pending) = state.pending.filter(|pending| pending.epoch == epoch) {
            state.pending = None;
            self.announce(&mut state, pending);
        }
    }

    /// Announces any staged hand-off at once. The bus calls it before it
    /// reports an end-of-stream: the end-of-stream has been rendered, so the
    /// track it ends has been heard, and the frontend must learn that it took
    /// over before it learns that it finished.
    pub(crate) fn complete_pending_handoff(&self) {
        let mut state = self.lock();
        if let Some(pending) = state.pending.take() {
            self.announce(&mut state, pending);
        }
    }

    fn handoff_stands(&self, epoch: u64) -> Option<gst::ClockTime> {
        self.lock()
            .pending
            .filter(|pending| pending.epoch == epoch)
            .map(|pending| pending.boundary())
    }
}

/// Watches `playbin` render up to the staged hand-off `epoch` and announces it
/// then. Holds neither the gate nor the pipeline alive, and gives up as soon as
/// the hand-off is withdrawn or the pipeline is stopped. A position the
/// pipeline keeps leaving unanswered while playing announces it anyway: an
/// unmeasurable render must not hold the frontend on the previous track.
pub(super) fn watch_render(
    gate: &Arc<SegmentGate>,
    playbin: glib::WeakRef<gst::Element>,
    epoch: u64,
) {
    let watched: Weak<SegmentGate> = Arc::downgrade(gate);
    let spawned = std::thread::Builder::new()
        .name("reprise-cue-handoff".into())
        .spawn(move || {
            let mut unanswered = 0;
            loop {
                let (Some(gate), Some(playbin)) = (watched.upgrade(), playbin.upgrade()) else {
                    return;
                };
                let Some(boundary) = gate.handoff_stands(epoch) else {
                    return;
                };
                let pause = match playbin.current_state() {
                    gst::State::Playing => {
                        let position = playbin.query_position::<gst::ClockTime>();
                        unanswered = if position.is_some() {
                            0
                        } else {
                            unanswered + 1
                        };
                        let heard = position.is_some_and(|position| position >= boundary);
                        if heard || unanswered > UNANSWERED_POLLS_TOLERATED {
                            gate.complete_handoff(epoch);
                            return;
                        }
                        RENDER_POLL_PLAYING
                    }
                    gst::State::Paused => RENDER_POLL_IDLE,
                    _ => return,
                };
                drop((gate, playbin));
                std::thread::sleep(pause);
            }
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "could not watch the CUE hand-off; announcing it now");
        gate.complete_handoff(epoch);
    }
}
