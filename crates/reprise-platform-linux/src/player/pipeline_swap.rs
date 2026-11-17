//! Builds the pipelines the player puts into service, and swaps them in.
//!
//! Every start of a track is a fresh pipeline (see `Player::play`), so building
//! one and putting it in the playbin slot is the common path of three callers:
//! `play`, which starts a track on it; `stop` (and a failed start), which put an
//! idle one in the slot; and the start watchdog, which begins a hung start over
//! on a new pipeline rather than taking the hung one through `Null` in place. A
//! hung pipeline can be stuck inside a kernel read that `Null` waits for, and
//! `Null` under the playbin lock on the main context would freeze every path
//! that wants either. So the pipeline that leaves the slot is never set to
//! `Null` here: it is retired (see `pipeline_queue.rs`), after its bus watch is
//! gone, so nothing it posts while it winds down reaches the frontend.
//!
//! The swap holds only clones of what the player shares, so the watchdog's
//! timer can own one without keeping a `Player` alive.

use gstreamer as gst;
use gstreamer::prelude::*;
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use reprise_core::playback::{AudioEffects, PlaybackError, PlayerEvent};

use super::pipeline_queue::{Answer, Asked, Pipeline};
use super::segment::SegmentHandle;
use super::start_watchdog::{is_hung, RestartRequest, Restarted};
use crate::crossfade::Transition;
use crate::gapless::{HandoffFlag, NextUri, PendingGain};
use crate::player_effects::{set_playbin_spectrum_messages, set_playbin_track_gain};
use crate::player_pipeline::{
    attach_bus_watch, build_pipeline, configure_download_buffering, CueCompletion,
};

/// What a track's start was, kept so the watchdog can begin it over without
/// reading it back from a pipeline that may be inside its own stuck state
/// change (`carry_start` used to read the URI of a hung `playbin3`).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct StartRecord {
    pub(super) uri: String,
    pub(super) live: bool,
    pub(super) gain_db: f64,
    /// The CUE track's stretch of the file; its start goes into `Paused`.
    pub(super) segment: Option<(i64, i64)>,
}

impl StartRecord {
    /// The state the pipeline's start asks for first: a CUE track's file is
    /// only set to paused, and the preroll completes the start.
    fn first_state(&self) -> gst::State {
        if self.segment.is_some() {
            gst::State::Paused
        } else {
            gst::State::Playing
        }
    }
}

/// The record of the start in service, `None` while the pipeline is idle.
/// Written under the playbin lock, and only ever locked after it.
pub(super) type StartRecordSlot = Arc<Mutex<Option<StartRecord>>>;

/// The answer to a start's state change.
pub(super) type StartAnswer = Answer<Result<gst::StateChangeSuccess, gst::StateChangeError>>;

/// A start that has been put in service.
pub(super) struct Started {
    /// The stream generation this start is current under.
    pub(super) generation: u64,
    /// The element of the pipeline now in the slot.
    pub(super) element: gst::Element,
    pub(super) answer: StartAnswer,
}

/// The shared state a pipeline is built from and installed into.
pub(super) struct PipelineSwap {
    pub(super) playbin: Arc<Mutex<Pipeline>>,
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
    pub(super) start_record: StartRecordSlot,
}

impl PipelineSwap {
    /// Builds a pipeline for the track `record` describes, with its bus watch.
    pub(super) fn build_for(
        &self,
        record: &StartRecord,
    ) -> Result<(Pipeline, gst::bus::BusWatchGuard), PlaybackError> {
        let (fresh, watch) = self.build()?;
        configure_start(fresh.element(), record)?;
        Ok((fresh, watch))
    }

    /// Puts `fresh`, built by [`Self::build_for`] for `record`, in the slot as
    /// the start of a track, and retires the pipeline it replaces. The start's
    /// state change is queued before the playbin lock is released, so whatever
    /// takes the lock next finds the start already on its way.
    ///
    /// Under the lock: the user's volume is carried over (a fresh playbin
    /// starts at its factory volume, 1.0); a CUE track's cut is installed;
    /// the start record is written; and the stream generation is bumped, once
    /// per start, whether it goes on to succeed or fail, so a fault of this
    /// start is never mistaken for the previous stream's by the frontend.
    pub(super) fn begin_start(
        &self,
        fresh: Pipeline,
        watch: gst::bus::BusWatchGuard,
        record: StartRecord,
    ) -> Started {
        let element = fresh.element().clone();
        let first_state = record.first_state();
        let (previous, generation, answer) = {
            let mut playbin = self.playbin.lock().unwrap_or_else(PoisonError::into_inner);
            // Read under the playbin lock: a concurrent `set_volume` has then
            // either already stored its value (seen here) or applies it to the
            // new element after the swap. Lock order playbin -> user_volume is
            // safe; no path nests them the other way.
            self.carry_volume(fresh.element());
            if let Some(segment) = record.segment {
                self.segments.begin_start(&record.uri, segment);
            }
            // The user asked for a track to play; a CUE start's first state is
            // only the means.
            fresh.set_asked(Asked::Playing);
            let answer = fresh.ask(move |element| element.set_state(first_state));
            let previous = std::mem::replace(&mut *playbin, fresh);
            self.install_watch(watch);
            *self
                .start_record
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = Some(record);
            let generation = self.stream_generation.fetch_add(1, Ordering::SeqCst) + 1;
            (previous, generation, answer)
        };
        previous.retire();
        Started {
            generation,
            element,
            answer,
        }
    }

    /// Replaces the pipeline whose element is `expected` (any, if `None`) with
    /// an idle one, and retires it. For `stop`, and for a start that failed:
    /// the track's pipeline goes the way of every retired pipeline, and what is
    /// left in the slot is a fresh one that has no source.
    ///
    /// Nothing is swapped, and `false` is returned, if the slot no longer holds
    /// `expected`: whoever took it out has retired it. `Err` means no idle
    /// pipeline could be built, and the current one is told to go to `Null` on
    /// its own queue instead.
    pub(super) fn replace_with_idle(
        &self,
        expected: Option<&gst::Element>,
    ) -> Result<bool, PlaybackError> {
        let (idle, watch) = match self.build() {
            Ok(built) => built,
            Err(error) => {
                tracing::warn!(%error, "could not build an idle pipeline; taking the current one to Null");
                // One critical section: the pipeline is told to go down, is
                // no longer asked to play, and has no start to begin over, so
                // a later `toggle_pause` does not start the stopped track
                // again on the pipeline that is still in the slot.
                let playbin = self.playbin.lock().unwrap_or_else(PoisonError::into_inner);
                playbin.set_asked(Asked::Idle);
                playbin.submit(|element| {
                    let _ = element.set_state(gst::State::Null);
                });
                self.start_record
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .take();
                return Err(error);
            }
        };
        let previous = {
            let mut playbin = self.playbin.lock().unwrap_or_else(PoisonError::into_inner);
            if expected.is_some_and(|expected| playbin.element() != expected) {
                return Ok(false);
            }
            self.carry_volume(idle.element());
            self.start_record
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take();
            let previous = std::mem::replace(&mut *playbin, idle);
            self.install_watch(watch);
            previous
        };
        previous.retire();
        Ok(true)
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
    /// and the fresh pipeline gets the start from the player's record of it,
    /// with the track gain as `set_current_gain_db` last made it, not from the
    /// hung pipeline, which may be inside a state change that never returns.
    ///
    /// The new pipeline starts into the state the user asked for: `Paused` if
    /// they paused during the start (a pause queued behind a start that blocks
    /// never ran), else the first state of the start. Its start is a job on its
    /// own queue (see below), so `Done` means the swap happened and the start
    /// was issued, not that it succeeded. `Err` therefore always means nothing
    /// was swapped: a pipeline that could not be built, or could not take the
    /// start over. A start that fails after the swap is reported by the new
    /// pipeline's own bus, whose watch is installed below, so the failure
    /// reaches the frontend once.
    ///
    /// The start is a job on the new pipeline's queue and not a state change
    /// made here: it opens the source, and a source blocked in `open()` (a dead
    /// mount, a FIFO nobody writes) keeps the change from returning. Waiting
    /// for it here would freeze the main context and every path that wants this
    /// lock. Queued under the lock, so a `stop` that takes it next finds the
    /// start already on its way and its `Null` ordered behind it.
    pub(super) fn restart_hung_start(
        &self,
        request: &RestartRequest<'_>,
    ) -> Result<Restarted, PlaybackError> {
        let (fresh, watch) = self.build()?;
        let retired = {
            let mut playbin = self.playbin.lock().unwrap_or_else(PoisonError::into_inner);
            let (_, current, pending) = playbin.element().state(gst::ClockTime::ZERO);
            let same_stream = self.stream_generation.load(Ordering::SeqCst) == request.generation;
            if !(request.still_armed)() || !same_stream || !is_hung(current, pending) {
                // `fresh` and its watch never left this scope; dropped unused.
                return Ok(Restarted::StoodDown);
            }
            let Some(record) = self
                .start_record
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
            else {
                return Ok(Restarted::StoodDown);
            };
            configure_start(fresh.element(), &record)?;
            self.carry_volume(fresh.element());
            let asked = playbin.asked();
            fresh.set_asked(asked);
            let target = if asked == Asked::Paused {
                gst::State::Paused
            } else {
                record.first_state()
            };
            fresh.submit(move |element| {
                if let Err(error) = element.set_state(target) {
                    tracing::warn!(%error, ?target, "the restarted start's state change failed");
                }
            });
            let retired = std::mem::replace(&mut *playbin, fresh);
            self.install_watch(watch);
            retired
        };
        retired.retire();
        Ok(Restarted::Done)
    }

    /// Builds a pipeline like the current one, with no track, and attaches its
    /// bus watch.
    pub(super) fn build(&self) -> Result<(Pipeline, gst::bus::BusWatchGuard), PlaybackError> {
        #[cfg(test)]
        if test_seam::build_fails() {
            return Err(PlaybackError::Backend(
                "test: the build was made to fail".into(),
            ));
        }
        let effects = self
            .effects
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        // From here on a failure drops the pipeline, which retires it.
        let fresh = build_pipeline(
            &effects,
            self.next_uri.clone(),
            self.handoff_pending.clone(),
            self.transition.clone(),
            self.stream_generation.clone(),
            self.pending_gain.clone(),
            self.segments.clone(),
        )?;
        set_playbin_spectrum_messages(
            fresh.element(),
            self.spectrum_enabled.load(Ordering::SeqCst),
        )?;
        let watch = attach_bus_watch(
            fresh.element(),
            self.on_event.clone(),
            self.handoff_pending.clone(),
            self.crossfading.clone(),
            self.spectrum_enabled.clone(),
            self.cava_stream_generation.clone(),
            CueCompletion {
                segments: self.segments.clone(),
                queue: fresh.queue(),
            },
        )?;
        #[cfg(test)]
        test_seam::built(fresh.element());
        Ok((fresh, watch))
    }

    fn carry_volume(&self, fresh: &gst::Element) {
        let user_volume = *self
            .user_volume
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        fresh.set_property("volume", user_volume);
    }

    /// Swapping the guard drops the replaced pipeline's bus watch, so nothing
    /// that pipeline posts on the way down reaches the handler; the caller
    /// retires it afterwards, with the lock released.
    ///
    /// Called with the playbin lock held, in the same critical section as the
    /// swap of the pipeline: a swap and its watch are one step. Done after the
    /// lock was released, a `play` that landed in between would have its fresh
    /// pipeline's watch overwritten by the watch of the pipeline it retired,
    /// and that track would never report its end. Lock order playbin ->
    /// bus_watch; no path nests them the other way, and dropping a guard only
    /// destroys a source.
    ///
    /// The bus watch is not the only thing a retired pipeline still carries:
    /// until its `Null` lands, which can be never on a dead source, its CAVA
    /// appsink callback, its gain-switch probe, its segment-boundary probe and
    /// its `about-to-finish` handler stay installed. The first three are
    /// harmless. Its pads go flushing as `Null` begins, so nothing more arrives
    /// for them to act on. A hard start has no `pending_gain` (it is `None`),
    /// so the gain switch has nothing to apply to the shared state. And no CUE
    /// handoff is staged on a start, so the boundary probe has none to
    /// complete. The `about-to-finish` handler is not harmless by itself, since
    /// a retired pipeline in its tail could still fire it and take the
    /// successor fed for the pipeline that replaced it, so it asks the
    /// pipeline's retirement first (`gapless::connect_about_to_finish`).
    fn install_watch(&self, watch: gst::bus::BusWatchGuard) {
        *self
            .bus_watch
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = watch;
    }
}

/// Gives `fresh` what a start sets: the download-buffering flags, the track gain
/// and the URI.
pub(super) fn configure_start(
    fresh: &gst::Element,
    record: &StartRecord,
) -> Result<(), PlaybackError> {
    configure_download_buffering(fresh, &record.uri, record.live)?;
    set_playbin_track_gain(fresh, record.gain_db)?;
    fresh.set_property("uri", record.uri.as_str());
    Ok(())
}

/// Test seam: lets a test reach every pipeline a swap builds before it starts,
/// the way it reaches the first one by hand, and make a build fail. A probe or a signal handler put on
/// the first pipeline does not follow a start to its fresh pipeline, and every
/// start builds one. Process-global, which is safe because every test that
/// swaps holds `AUDIO_SINK_TEST_LOCK` for its whole life.
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
        FAIL_BUILDS.store(0, Ordering::SeqCst);
    }

    static FAIL_BUILDS: AtomicUsize = AtomicUsize::new(0);

    /// Makes the next `builds` calls of [`PipelineSwap::build`] fail, until
    /// [`clear`].
    pub(in crate::player) fn fail_next_builds(builds: usize) {
        FAIL_BUILDS.store(builds, Ordering::SeqCst);
    }

    /// Whether this build is one a test made fail; counts it off.
    pub(super) fn build_fails() -> bool {
        FAIL_BUILDS
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                left.checked_sub(1)
            })
            .is_ok()
    }

    pub(super) fn built(fresh: &gst::Element) {
        let hooks = BUILT.lock().unwrap_or_else(PoisonError::into_inner).clone();
        for hook in hooks {
            hook(fresh);
        }
    }
}
