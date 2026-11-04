//! Replaces the player's playbin with a freshly built one.
//!
//! Two callers need the same sequence: `Player::rebuild_playbin`, the recovery
//! from a wedged pipeline, and the start watchdog, which begins a hung start
//! over on a new pipeline rather than taking the hung one through `Null` in
//! place. A hung pipeline can be stuck inside a kernel read that `Null` waits
//! for, and `Null` under the playbin lock on the main context would freeze
//! every path that wants either. So the retired pipeline is never set to `Null`
//! here: it is handed to [`super::reaper`], after its bus watch is gone, so
//! nothing it posts while it winds down reaches the frontend.
//!
//! The swap holds only clones of what the player shares, so the watchdog's
//! timer can own one without keeping a `Player` alive.

use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use reprise_core::playback::{AudioEffects, PlaybackError, PlayerEvent};

use super::reaper::retire_pipeline;
use super::segment::SegmentHandle;
use super::start_watchdog::{is_hung, RestartRequest, Restarted};
use crate::crossfade::Transition;
use crate::gapless::{HandoffFlag, NextUri, PendingGain};
use crate::player_effects::{set_playbin_spectrum_messages, TRACK_GAIN_NAME};
use crate::player_pipeline::{attach_bus_watch, build_playbin, configure_download_buffering};

/// A local file start is never live; only those are begun over.
const RESTARTED_START_IS_LIVE: bool = false;

/// The shared state a replacement pipeline is built from and installed into.
pub(super) struct PipelineSwap {
    pub(super) playbin: Arc<Mutex<gst::Element>>,
    /// Held for its `Drop` side effect (removes the bus watch) and replaced
    /// wholesale by every swap, since a `BusWatchGuard` is tied to the specific
    /// `Bus` (and thus element) it was created for. `Arc<Mutex<_>>` rather than
    /// `RefCell` because the crossfade ramp thread swaps it from off-thread when
    /// it promotes the incoming pipeline (see `crossfade.rs`).
    pub(super) bus_watch: Arc<Mutex<gst::bus::BusWatchGuard>>,
    pub(super) on_event: Arc<dyn Fn(PlayerEvent) + Send + Sync>,
    pub(super) effects: Arc<Mutex<AudioEffects>>,
    pub(super) next_uri: NextUri,
    pub(super) pending_gain: PendingGain,
    pub(super) handoff_pending: HandoffFlag,
    pub(super) transition: Transition,
    pub(super) crossfading: Arc<AtomicBool>,
    pub(super) user_volume: Arc<Mutex<f64>>,
    pub(super) spectrum_enabled: Arc<AtomicBool>,
    pub(super) cava_stream_generation: Arc<AtomicU64>,
    pub(super) stream_generation: Arc<AtomicU64>,
    pub(super) segments: SegmentHandle,
}

impl PipelineSwap {
    /// Discards the current playbin for a freshly built one, for the wedged-
    /// pipeline recovery (see `Player::play`). The caller has already cleared
    /// the transition slots, as every hard restart does.
    pub(super) fn replace(&self) -> Result<(), PlaybackError> {
        let (fresh, watch) = self.build()?;
        let retired = {
            let mut playbin = self.playbin.lock().unwrap_or_else(PoisonError::into_inner);
            // A fresh playbin starts at its factory volume (1.0), so carry the
            // user's volume over. Read under the playbin lock: a concurrent
            // `set_volume` has then either already stored its value (seen here)
            // or applies it to the new element after the swap. Lock order
            // playbin -> user_volume is safe; no path nests them the other way.
            self.carry_volume(&fresh);
            std::mem::replace(&mut *playbin, fresh)
        };
        self.retire(retired, watch);
        Ok(())
    }

    /// Begins the hung start of the current pipeline over on a fresh one.
    ///
    /// The watchdog looked at the start without the playbin lock held across
    /// what follows, so everything it decided on is checked again here, under
    /// the lock, before anything is touched: the watch is still the one that
    /// was armed (`stop` and a dropped player disarm it ahead of taking the
    /// lock), the stream is still the one it was armed for (a gapless step or
    /// a crossfade promotion replace a start without disarming), and the
    /// pipeline is still hung. If not, nothing is swapped.
    ///
    /// The stream generation is not bumped (the restart is the same start), the
    /// segment gate is not reset (a CUE start's pending seek must survive),
    /// and the fresh pipeline gets the old one's URI, gain and download
    /// flags before it enters the pending state of the one it replaces.
    pub(super) fn restart_hung_start(
        &self,
        request: &RestartRequest<'_>,
    ) -> Result<Restarted, PlaybackError> {
        let (fresh, watch) = self.build()?;
        let (retired, started) = {
            let mut playbin = self.playbin.lock().unwrap_or_else(PoisonError::into_inner);
            let (_, current, pending) = playbin.state(gst::ClockTime::ZERO);
            let same_stream = self.stream_generation.load(Ordering::SeqCst) == request.generation;
            if !(request.still_armed)() || !same_stream || !is_hung(current, pending) {
                // `fresh` and its watch never left this scope; dropped unused.
                return Ok(Restarted::StoodDown);
            }
            carry_start(&playbin, &fresh)?;
            self.carry_volume(&fresh);
            let retired = std::mem::replace(&mut *playbin, fresh.clone());
            // Under the lock like `try_play`'s start, but on the fresh pipeline:
            // a failing change has put its error on its bus, whose watch is
            // installed below, so the failure reaches the frontend once.
            (retired, fresh.set_state(pending))
        };
        self.retire(retired, watch);
        started
            .map(|_| Restarted::Done)
            .map_err(|error| PlaybackError::Backend(format!("GStreamer: {error}")))
    }

    /// Builds a pipeline like the current one and attaches its bus watch.
    fn build(&self) -> Result<(gst::Element, gst::bus::BusWatchGuard), PlaybackError> {
        let effects = self
            .effects
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let fresh = build_playbin(
            &effects,
            self.next_uri.clone(),
            self.handoff_pending.clone(),
            self.transition.clone(),
            self.stream_generation.clone(),
            self.pending_gain.clone(),
            self.segments.clone(),
        )?;
        set_playbin_spectrum_messages(&fresh, self.spectrum_enabled.load(Ordering::SeqCst))?;
        let watch = attach_bus_watch(
            &fresh,
            self.on_event.clone(),
            self.handoff_pending.clone(),
            self.crossfading.clone(),
            self.spectrum_enabled.clone(),
            self.cava_stream_generation.clone(),
            self.segments.clone(),
        )?;
        #[cfg(test)]
        test_seam::built(&fresh);
        Ok((fresh, watch))
    }

    fn carry_volume(&self, fresh: &gst::Element) {
        let user_volume = *self
            .user_volume
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        fresh.set_property("volume", user_volume);
    }

    /// Swapping the guard drops the retired pipeline's bus watch, and only then
    /// does the pipeline go to the reaper: nothing it posts on the way down
    /// reaches the handler.
    fn retire(&self, retired: gst::Element, watch: gst::bus::BusWatchGuard) {
        *self
            .bus_watch
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = watch;
        retire_pipeline(retired);
    }
}

/// Gives `fresh` what a start set on `old`: the URI, the download-buffering
/// flags and the track gain as the old pipeline holds it now, which a gain
/// refreshed after the start has already updated.
fn carry_start(old: &gst::Element, fresh: &gst::Element) -> Result<(), PlaybackError> {
    let uri = old
        .property::<Option<String>>("uri")
        .ok_or_else(|| PlaybackError::Backend("GStreamer: the hung start has no URI".into()))?;
    configure_download_buffering(fresh, &uri, RESTARTED_START_IS_LIVE)?;
    track_gain(fresh)?.set_property("volume", track_gain(old)?.property::<f64>("volume"));
    fresh.set_property("uri", uri.as_str());
    Ok(())
}

/// The element that applies the track's gain, inside the audio filter.
fn track_gain(playbin: &gst::Element) -> Result<gst::Element, PlaybackError> {
    playbin
        .property::<Option<gst::Element>>("audio-filter")
        .and_then(|filter| filter.downcast::<gst::Bin>().ok())
        .and_then(|bin| bin.by_name(TRACK_GAIN_NAME))
        .ok_or_else(|| PlaybackError::Backend("GStreamer: audio filter has no track gain".into()))
}

/// Test seam: lets a test reach every pipeline a swap builds before it starts,
/// the way it reaches the first one by hand. A probe or a signal handler put on
/// the first pipeline does not follow a start over to its replacement.
/// Process-global, which is safe because every test that swaps holds
/// `AUDIO_SINK_TEST_LOCK` for its whole life.
#[cfg(test)]
pub(super) mod test_seam {
    use super::*;

    type Hook = Arc<dyn Fn(&gst::Element) + Send + Sync>;

    static BUILT: Mutex<Vec<Hook>> = Mutex::new(Vec::new());

    /// Runs `hook` on every pipeline a swap builds from now on, until
    /// [`clear`].
    pub(in crate::player) fn on_pipeline_built(hook: Hook) {
        BUILT
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(hook);
    }

    /// Removes every hook.
    pub(in crate::player) fn clear() {
        BUILT.lock().unwrap_or_else(PoisonError::into_inner).clear();
    }

    pub(super) fn built(fresh: &gst::Element) {
        let hooks = BUILT.lock().unwrap_or_else(PoisonError::into_inner).clone();
        for hook in hooks {
            hook(fresh);
        }
    }
}
