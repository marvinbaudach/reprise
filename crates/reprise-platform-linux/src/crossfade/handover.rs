//! How a crossfade ends: the fade giving way to a plain track change, and the
//! secondary's promotion to the primary pipeline.
//!
//! Neither ever sets a pipeline to `Null` itself. A pipeline that leaves service
//! is retired (see `player/pipeline_queue.rs`), because a source blocked in the
//! kernel holds its `Null` up, and these run on the ramp thread, where the
//! outgoing track's end-of-stream is waiting on them, and (`promote`) next to
//! the playbin lock, which every main-context path wants.

use super::*;
use crate::player_pipeline::{attach_bus_watch, CueCompletion};

impl CrossfadeEngine {
    /// Takes the secondary out of the incoming slot and retires it, if the slot
    /// still holds `secondary`. An empty slot, or another pipeline in it, means
    /// an abort (or a newer fade) already took it: nothing is left to do.
    pub(super) fn retire_incoming(&self, secondary: &gst::Element) {
        let taken = {
            let mut slot = self.incoming.lock().unwrap_or_else(PoisonError::into_inner);
            if slot
                .as_ref()
                .is_some_and(|pipeline| pipeline.element() == secondary)
            {
                slot.take()
            } else {
                None
            }
        };
        if let Some(pipeline) = taken {
            pipeline.retire();
        }
    }

    /// Gives up the fade: the secondary, if there is one in the slot, is retired
    /// and leaves it, and the guard is released so the outgoing track's
    /// end-of-stream is reported again. (A secondary that was never published
    /// is the caller's, and is retired when it goes out of scope.) The ramp
    /// never ran, so the outgoing track gets back its full volume, or the one
    /// the user chose meanwhile. An end-of-stream the outgoing track reached
    /// while the fade waited is reported here, once, because the guard made the
    /// bus watch drop it. A fade that a manual jump already superseded is left
    /// alone: the jump cleaned up, and its guard may belong to a newer fade by
    /// now.
    pub(super) fn give_way(
        &self,
        secondary: Option<&gst::Element>,
        outgoing_end: &EosCatch,
        my_generation: u64,
    ) {
        if let Some(secondary) = secondary {
            self.retire_incoming(secondary);
        }
        if self.generation.load(Ordering::SeqCst) != my_generation {
            return;
        }
        // Order matters. With the guard down first, an end-of-stream that
        // arrives from here on is either caught (and reported below) or
        // reaches the bus watch, which reports it itself.
        self.crossfading.store(false, Ordering::SeqCst);
        // `set_volume` only stores its target while the guard is up, and the
        // ramp that would have applied it never ran. A `set_volume` after the
        // guard went down applies itself; one before it is read here.
        self.restore_user_volume();
        #[cfg(test)]
        test_seam::before_release();
        // A `play()` that landed after the check above started a track of its
        // own; the end this fade caught belongs to the one it replaced.
        let caught = outgoing_end.release();
        if caught && self.generation.load(Ordering::SeqCst) == my_generation {
            tracing::debug!("crossfade: the outgoing track ended during the wait; reporting it");
            self.segments.complete_pending_handoff();
            (self.on_event)(PlayerEvent::TrackFinished);
        }
    }

    /// Applies the user's volume to the primary, as `Player::abort_crossfade`
    /// does after it cancels a fade.
    fn restore_user_volume(&self) {
        let user_volume = *self
            .user_volume
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.playbin
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .element()
            .set_property("volume", user_volume);
    }

    /// Promotes the finished secondary to the new primary `playbin`: attach a
    /// fresh bus watch, swap atomically under the `playbin` lock, restore the
    /// target volume, replace the old watch, retire the old pipeline (after the
    /// lock is released) and emit `AdvancedToNext`.
    pub(super) fn promote(
        self,
        secondary: &gst::Element,
        outgoing_end: &EosCatch,
        my_generation: u64,
    ) {
        // Before attaching the watch, drop any *stale* bus messages (for
        // instance an EOS that a very short successor already posted during
        // the fade). With real songs nothing is queued here; the drain only
        // guards against stale events from tests with tiny fixtures — the new
        // hand-off (`AdvancedToNext`) is emitted manually anyway, not derived
        // from the secondary's StreamStart.
        if let Some(bus) = secondary.bus() {
            while bus.pop().is_some() {}
        }
        let queue = self
            .incoming
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .filter(|pipeline| pipeline.element() == secondary)
            .map(Pipeline::queue);
        // An empty slot: an abort took the secondary and has cleaned up.
        let Some(queue) = queue else {
            return;
        };
        let new_watch = match attach_bus_watch(
            secondary,
            self.on_event.clone(),
            self.handoff_pending.clone(),
            self.crossfading.clone(),
            self.spectrum_enabled.clone(),
            self.cava_stream_generation.clone(),
            CueCompletion {
                segments: self.segments.clone(),
                queue,
            },
        ) {
            Ok(watch) => watch,
            Err(error) => {
                tracing::warn!(%error, "crossfade: could not attach bus watch to promoted pipeline");
                // The outgoing track plays on: it needs its end-of-stream
                // reported, and its volume back.
                self.give_way(Some(secondary), outgoing_end, my_generation);
                return;
            }
        };
        let user_volume = *self
            .user_volume
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let old = {
            let mut primary = self.playbin.lock().unwrap_or_else(PoisonError::into_inner);
            // Last abort check under the lock: if the fade was aborted in the
            // meantime, drop the freshly attached watch, retire the secondary
            // and leave the primary alone (the abort path has cleaned up).
            if self.generation.load(Ordering::SeqCst) != my_generation {
                drop(new_watch);
                self.retire_incoming(secondary);
                return;
            }
            let promoted = {
                let mut slot = self.incoming.lock().unwrap_or_else(PoisonError::into_inner);
                if slot
                    .as_ref()
                    .is_some_and(|pipeline| pipeline.element() == secondary)
                {
                    slot.take()
                } else {
                    None
                }
            };
            // An abort between the check and here took the secondary.
            let Some(promoted) = promoted else {
                drop(new_watch);
                return;
            };
            let old = std::mem::replace(&mut *primary, promoted);
            // The watch swaps with the pipeline, under the same lock: a `play`
            // that lands after it must find its own watch where it put it.
            // Replaces the old primary's guard, whose drop removes the old watch.
            *self
                .bus_watch
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = new_watch;
            // Bumped here, under the same `playbin` lock as the swap, not when
            // the secondary silently started minutes/seconds earlier: until
            // this instant `self.playbin` (and thus every `Position` tick the
            // ticker reads through it) still describes the *outgoing* track,
            // so bumping any earlier would mislabel those ticks as belonging
            // to a stream that has not actually taken over yet. Taking the
            // lock provides the happens-before edge that guarantees no ticker
            // read after this point can observe the old generation.
            self.stream_generation.fetch_add(1, Ordering::SeqCst);
            secondary.set_property("volume", user_volume);
            old
        };
        // Retired with the lock free: its `Null` can take as long as a stuck
        // source likes.
        old.retire();
        self.crossfading.store(false, Ordering::SeqCst);
        (self.on_event)(PlayerEvent::AdvancedToNext);
    }
}
