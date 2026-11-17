use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use reprise_core::library::settings::{TrackTransition, CROSSFADE_SECONDS_DEFAULT};
use reprise_core::playback::{
    AudioEffects, PlaybackBackend, PlaybackError, PlaybackItem, PlaybackState, PlayerEvent,
    StreamEvent, StreamGeneration,
};

use crate::crossfade::{CrossfadeEngine, IncomingSlot, Transition};
use crate::gapless::{HandoffFlag, NextUri, PendingGain, QueuedTrack};
use crate::player_effects::{
    set_playbin_spectrum_messages, set_playbin_track_gain, update_existing_audio_filter,
};
use crate::player_pipeline::{
    attach_bus_watch, build_pipeline, path_to_uri, validated_playback_uri, CueCompletion,
};
use pipeline_queue::Pipeline;
use pipeline_swap::PipelineSwap;
use segment::{SegmentGate, SegmentHandle};
use start_watchdog::StartWatchdog;

pub(crate) mod pipeline_queue;
mod pipeline_swap;
mod reaper;
pub(crate) mod segment;
mod start;
mod start_watchdog;
mod successor;
mod transport;

/// Default playback volume before the user ever moves the slider — full scale,
/// matching `playbin3`'s own `volume` property default. Also the value the
/// crossfade ramp restores the promoted pipeline to.
const DEFAULT_VOLUME: f64 = 1.0;

const POSITION_TICK_INTERVAL: Duration = Duration::from_millis(500);

/// How long `play` and `seek_to` wait on the main context for the answer of a
/// pipeline's state change or seek (decision 2 of the transport plan): a fast
/// failure such as a missing file stays a synchronous `Err`, and a source that
/// blocks in the kernel is left to the watchdog after this long. Well under the
/// 500 ms the freeze tests allow the main context to stand still, a pipeline
/// build included; a healthy start answers in about 10 ms.
pub(crate) const START_ANSWER_WAIT: Duration = Duration::from_millis(250);

/// While a gapless hand-off is pending, `playbin3` already answers
/// `query_duration` for the *next* stream while `query_position` still measures
/// the current one. Reporting that pair would make the UI compute a fraction of
/// `duration_old / duration_new` and jump the playhead backwards, so the last
/// duration seen for the running stream is held until `StreamStart` lands.
///
/// Two pairings are deliberately left unguarded. A tick can land after the
/// internal stream switch but before GLib dispatches `StreamStart` and clears
/// the flag, pairing the new stream's near-zero position with the held
/// duration — but the correct pairing puts the playhead at the very start of
/// the track too, so there is nothing visible to gain by catching it. And
/// `last_stable_ms` is never reset per track, so a track whose entire tenure
/// is shorter than one 500 ms tick would still be described by an older
/// track's duration; music files are never that short, and guarding it would
/// cost more complexity than the case is worth.
fn reported_duration_ms(queried_ms: i64, last_stable_ms: i64, handoff_pending: bool) -> i64 {
    if handoff_pending && last_stable_ms > 0 {
        last_stable_ms
    } else {
        queried_ms
    }
}

pub struct Player {
    /// The live GStreamer pipeline, with the queue every state change and
    /// seek of it goes through (see `player/pipeline_queue.rs`).
    /// `Arc<Mutex<_>>`, not a plain field: the position-ticker thread (spawned
    /// in `new`) needs to read whichever element is *current* on every tick, and
    /// every start swaps in a freshly built pipeline (see `play`'s doc comment).
    /// A plain field (or a value captured once by the ticker thread at spawn
    /// time) would go stale at the first start, silently freezing position
    /// ticks for the rest of the process's life.
    playbin: Arc<Mutex<Pipeline>>,
    on_event: Arc<dyn Fn(PlayerEvent) + Send + Sync + 'static>,
    effects: Arc<Mutex<AudioEffects>>,
    /// Gapless slot: the URI pre-fed via `set_next`, consumed by the
    /// `about-to-finish` handler installed in `build_playbin` (Gapless mode) or
    /// by the position ticker when it starts a crossfade (Crossfade mode).
    /// Shared across threads — see `gapless.rs` / `crossfade.rs`.
    next_uri: NextUri,
    pending_gain: PendingGain,
    /// Set by the `about-to-finish` handler when it hands off a pre-fed URI,
    /// cleared by the bus watch's `StreamStart` handling — together they
    /// distinguish a gapless handoff (emit `AdvancedToNext`) from an ordinary
    /// first-track `StreamStart` (emit nothing). See `gapless.rs`.
    handoff_pending: HandoffFlag,
    /// The active `(mode, crossfade_seconds)`. Default `(Gapless, DEFAULT)` so
    /// the pipeline behaves gaplessly before the frontend ever calls
    /// `set_transition`. Read by the `about-to-finish` handler and the ticker.
    transition: Transition,
    /// `true` while a crossfade ramp is in flight — see `crossfade.rs`.
    crossfading: Arc<AtomicBool>,
    /// The volume the user last requested (the ramp's target/ceiling). Tracked
    /// separately from the pipeline's live `volume` property because that
    /// property is transiently driven by the ramp during a crossfade.
    user_volume: Arc<Mutex<f64>>,
    /// Bumped on every crossfade start and every abort; lets an in-flight ramp
    /// thread detect it has been superseded/cancelled and terminate safely.
    fade_generation: Arc<AtomicU64>,
    /// The incoming secondary pipeline during a crossfade, so an abort can
    /// silence it immediately. See `crossfade.rs`.
    incoming: IncomingSlot,
    spectrum_enabled: Arc<AtomicBool>,
    /// Bumped synchronously on every `play`/`play_uri` (`play_resolved_uri`) or
    /// gapless/crossfade hand-off — the `PlaybackBackend` "Stream
    /// generations" contract. Stamped onto events as a `StreamEvent` for
    /// `new_with_generation` consumers.
    stream_generation: Arc<AtomicU64>,
    /// The stretch of the file a CUE track covers, while one plays — see
    /// `player/segment.rs`. Shared with the position ticker.
    segments: SegmentHandle,
    /// Starts a local file over when `playbin3` hangs in READY — see
    /// `player/start_watchdog.rs`.
    start_watchdog: StartWatchdog,
    /// Builds the fresh pipelines every start, `stop` and the watchdog's restart
    /// put in the playbin slot — see `player/pipeline_swap.rs`.
    swap: Arc<PipelineSwap>,
}

impl Player {
    /// Creates a new player and starts its background bus watch and position
    /// ticker. `on_event` is invoked (from either the GLib bus-watch context
    /// or the ticker thread) whenever a `PlayerEvent` occurs; wrapped in an
    /// `Arc` so both can share it. Delivers plain, untagged events, unchanged
    /// from before; a consumer that needs to discard stale events across a
    /// stream boundary should use [`Player::new_with_generation`] instead.
    pub fn new(
        on_event: Box<dyn Fn(PlayerEvent) + Send + Sync + 'static>,
    ) -> Result<Self, PlaybackError> {
        let on_event: Arc<dyn Fn(PlayerEvent) + Send + Sync> = Arc::from(on_event);
        Self::build(on_event, Arc::new(AtomicU64::new(0)))
    }

    /// Identical to [`Player::new`], except every emitted event is paired
    /// with the [`StreamGeneration`] current the instant it was produced (see
    /// [`StreamEvent`]) — tagged once, in the closure built here, around the
    /// plain `Fn(PlayerEvent)` every emission site still calls.
    pub fn new_with_generation(
        on_event: Box<dyn Fn(StreamEvent) + Send + Sync + 'static>,
    ) -> Result<Self, PlaybackError> {
        let stream_generation = Arc::new(AtomicU64::new(0));
        let tagging_generation = stream_generation.clone();
        let on_event: Arc<dyn Fn(PlayerEvent) + Send + Sync> = Arc::new(move |event| {
            let generation = StreamGeneration::from(tagging_generation.load(Ordering::SeqCst));
            on_event(StreamEvent { generation, event });
        });
        Self::build(on_event, stream_generation)
    }

    /// Shared construction logic behind `new`/`new_with_generation`.
    /// `stream_generation` is owned by the caller so both reuse the same
    /// bump sites (`play_resolved_uri`, the gapless hand-off, the crossfade promotion).
    fn build(
        on_event: Arc<dyn Fn(PlayerEvent) + Send + Sync>,
        stream_generation: Arc<AtomicU64>,
    ) -> Result<Self, PlaybackError> {
        gst::init().map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))?;

        let effects = Arc::new(Mutex::new(AudioEffects::default()));
        let next_uri: NextUri = Arc::new(Mutex::new(None));
        let pending_gain: PendingGain = Arc::new(Mutex::new(None));
        let handoff_pending: HandoffFlag = Arc::new(AtomicBool::new(false));
        // Default (Gapless, DEFAULT): the pipeline behaves gaplessly until the
        // frontend calls `set_transition`, keeping the Phase A gapless tests
        // (which never call it) green.
        let transition: Transition = Arc::new(Mutex::new((
            TrackTransition::Gapless,
            CROSSFADE_SECONDS_DEFAULT,
        )));
        let crossfading = Arc::new(AtomicBool::new(false));
        let user_volume = Arc::new(Mutex::new(DEFAULT_VOLUME));
        let fade_generation = Arc::new(AtomicU64::new(0));
        let incoming: IncomingSlot = Arc::new(Mutex::new(None));
        let spectrum_enabled = Arc::new(AtomicBool::new(false));
        let cava_stream_generation = Arc::new(AtomicU64::new(0));
        let segments = SegmentGate::new(on_event.clone(), stream_generation.clone());

        let playbin = build_pipeline(
            &effects.lock().unwrap_or_else(PoisonError::into_inner),
            next_uri.clone(),
            handoff_pending.clone(),
            transition.clone(),
            stream_generation.clone(),
            pending_gain.clone(),
            segments.clone(),
        )?;
        let bus_watch = attach_bus_watch(
            playbin.element(),
            on_event.clone(),
            handoff_pending.clone(),
            crossfading.clone(),
            spectrum_enabled.clone(),
            cava_stream_generation.clone(),
            CueCompletion {
                segments: segments.clone(),
                queue: playbin.queue(),
            },
        )?;
        let playbin = Arc::new(Mutex::new(playbin));
        let bus_watch = Arc::new(Mutex::new(bus_watch));
        let swap = Arc::new(PipelineSwap {
            playbin: playbin.clone(),
            bus_watch: bus_watch.clone(),
            on_event: on_event.clone(),
            effects: effects.clone(),
            next_uri: next_uri.clone(),
            pending_gain: pending_gain.clone(),
            handoff_pending: handoff_pending.clone(),
            transition: transition.clone(),
            crossfading: crossfading.clone(),
            user_volume: user_volume.clone(),
            spectrum_enabled: spectrum_enabled.clone(),
            cava_stream_generation: cava_stream_generation.clone(),
            stream_generation: stream_generation.clone(),
            segments: segments.clone(),
            start_record: Arc::new(Mutex::new(None)),
        });
        let restarting = swap.clone();
        let start_watchdog = StartWatchdog::new(
            playbin.clone(),
            stream_generation.clone(),
            on_event.clone(),
            Arc::new(move |request| restarting.restart_hung_start(request)),
        );

        let engine = CrossfadeEngine {
            playbin: playbin.clone(),
            bus_watch: bus_watch.clone(),
            on_event: on_event.clone(),
            effects: effects.clone(),
            next_uri: next_uri.clone(),
            pending_gain: pending_gain.clone(),
            handoff_pending: handoff_pending.clone(),
            transition: transition.clone(),
            crossfading: crossfading.clone(),
            user_volume: user_volume.clone(),
            generation: fade_generation.clone(),
            incoming: incoming.clone(),
            spectrum_enabled: spectrum_enabled.clone(),
            cava_stream_generation: cava_stream_generation.clone(),
            stream_generation: stream_generation.clone(),
            segments: segments.clone(),
        };

        // Position ticker: report position + duration every 500 ms while
        // playing, and — in Crossfade mode — start the overlapping fade once the
        // position enters the last `crossfade_seconds` window (see
        // `CrossfadeEngine::maybe_start`). Reads whichever element is current at
        // each tick (see the `playbin` field's doc comment), holding the mutex
        // only long enough to clone the `gst::Element` handle out (a cheap
        // refcount bump) — the actual state/position queries run outside the lock.
        let ticker = engine.clone();
        let ticker_segments = segments.clone();
        std::thread::spawn(move || {
            let mut last_stable_duration_ms = 0;
            loop {
                std::thread::sleep(POSITION_TICK_INTERVAL);
                let element = {
                    let guard = ticker
                        .playbin
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    guard.element().clone()
                };
                if element.current_state() == gst::State::Playing {
                    let position_ms = element
                        .query_position::<gst::ClockTime>()
                        .map_or(0, |t| t.mseconds() as i64);
                    let queried_duration_ms = element
                        .query_duration::<gst::ClockTime>()
                        .map_or(0, |t| t.mseconds() as i64);
                    let handoff_pending = ticker.handoff_pending.load(Ordering::SeqCst);
                    let whole_file = || {
                        let duration_ms = reported_duration_ms(
                            queried_duration_ms,
                            last_stable_duration_ms,
                            handoff_pending,
                        );
                        (position_ms, duration_ms)
                    };
                    let (position_ms, duration_ms) =
                        ticker_segments.send_tick(position_ms, queried_duration_ms, whole_file);
                    if !handoff_pending && queried_duration_ms > 0 {
                        last_stable_duration_ms = queried_duration_ms;
                    }
                    ticker.maybe_start(position_ms, duration_ms);
                }
            }
        });

        Ok(Self {
            playbin,
            on_event,
            effects,
            next_uri,
            pending_gain,
            handoff_pending,
            transition,
            crossfading,
            user_volume,
            fade_generation,
            incoming,
            spectrum_enabled,
            stream_generation,
            segments,
            start_watchdog,
            swap,
        })
    }

    /// Aborts any in-flight crossfade cleanly: bumps the fade generation (so the
    /// ramp thread notices it is superseded and terminates without touching the
    /// now-discarded elements), clears the `crossfading` guard, retires the
    /// incoming secondary pipeline (silencing it without waiting for its
    /// `Null`, which a stuck source can hold up), and restores the (outgoing)
    /// primary's `volume` to the user's target — the ramp may have faded it down
    /// partway, and whatever plays next on it must be at full requested volume.
    /// Idempotent and safe to call when no crossfade is running.
    fn abort_crossfade(&self) {
        self.fade_generation.fetch_add(1, Ordering::SeqCst);
        self.crossfading.store(false, Ordering::SeqCst);
        let secondary = self
            .incoming
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(secondary) = secondary {
            secondary.retire();
        }
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

    /// Clears both transition slots: the gapless pre-fed successor *and* any
    /// in-flight crossfade. Called on every hard restart (`play`, `stop`) — a pre-fed/overlapping successor is only valid
    /// relative to the track it was queued behind.
    fn reset_transition(&self) {
        self.abort_crossfade();
        self.reset_gapless();
        self.segments.reset();
    }

    /// Clears the gapless slot and the handoff flag. Called on every manual
    /// jump (`play`): a pre-fed successor is only valid
    /// relative to the track it was queued behind, so any hard restart
    /// invalidates it (the frontend re-feeds afterwards). Idempotent.
    fn reset_gapless(&self) {
        *self
            .next_uri
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        self.handoff_pending.store(false, Ordering::SeqCst);
        *self
            .pending_gain
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
    }
}

impl PlaybackBackend for Player {
    /// Starts playback of `path`.
    ///
    /// ## A fresh pipeline per start
    ///
    /// Every start builds a new `playbin3`, puts it in the slot and retires the
    /// previous one (`pipeline_swap.rs`, `pipeline_queue.rs`). Two reasons.
    /// A source that is stuck in the kernel (a dead NFS/SMB mount) holds the
    /// old pipeline's `Null` and its state lock for as long as it likes, and
    /// nothing on the main context may wait for that. And empirically (headless
    /// `fakesink` E2E, a deleted-file scenario) a `playbin3` whose
    /// `set_state(Playing)` failed once fails every later `Playing` on that same
    /// instance, even for a valid file; a pipeline no start ever sees twice has
    /// nothing to heal, which retired the wedged-pipeline recovery (rebuild and
    /// retry once) this method used to have.
    ///
    /// ## What `play` waits for
    ///
    /// The start itself is a job on the new pipeline's queue. `play` waits at
    /// most [`START_ANSWER_WAIT`] for its answer. A fast failure such as a
    /// missing file answers within a millisecond and stays a synchronous `Err`
    /// (the frontend's `Ok` side effects, scrobble and history, must never run
    /// for a file that never plays); the failed pipeline is retired with its bus
    /// watch removed, so its own error never reaches the frontend a second time.
    /// A start that has not answered by then is `Ok`, left to the watchdog, and
    /// a late failure arrives on the bus under the new stream generation.
    fn play(&self, item: PlaybackItem<'_>) -> Result<(), PlaybackError> {
        let uri = path_to_uri(item.path)?;
        self.play_resolved_uri(&uri, item.path, false, item.gain_db, item.segment)
    }

    fn play_uri(&self, uri: &str) -> Result<(), PlaybackError> {
        let uri = validated_playback_uri(uri)?;
        self.play_resolved_uri(&uri, uri.as_str(), false, 0.0, None)
    }

    fn play_live_uri(&self, uri: &str) -> Result<(), PlaybackError> {
        let uri = validated_playback_uri(uri)?;
        self.play_resolved_uri(&uri, uri.as_str(), true, 0.0, None)
    }

    fn toggle_pause(&self) -> Result<PlaybackState, PlaybackError> {
        self.toggle_pause_queued()
    }

    /// Seeks within the current track. A CUE track's `position_ms` is its
    /// own: the seek lands sample-accurately inside the track's stretch of
    /// the file. A whole file seeks to the nearest key unit, as it always has.
    /// Waits at most [`START_ANSWER_WAIT`] for the answer; a seek that has not
    /// been answered by then is an `Err` and stays queued (see `transport.rs`).
    fn seek_to(&self, position_ms: i64) -> Result<(), PlaybackError> {
        self.seek_queued(position_ms)
    }

    /// Sets the playback volume and remembers it as the crossfade ramp's target.
    /// During a crossfade the pipeline's live `volume` is driven by the ramp, so
    /// we only update the stored target (the ramp reads it each step and scales
    /// both pipelines to it); outside a crossfade we apply it to the pipeline
    /// directly. Either way the *user's* intended volume is the source of truth.
    fn set_volume(&self, volume: f64) {
        let volume = volume.clamp(0.0, 1.0);
        *self
            .user_volume
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = volume;
        if self.crossfading.load(Ordering::SeqCst) {
            return;
        }
        self.playbin
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .element()
            .set_property("volume", volume);
    }

    fn set_audio_effects(&self, effects: AudioEffects) -> Result<(), PlaybackError> {
        let mut current_effects = self
            .effects
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let playbin = self
            .playbin
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        update_existing_audio_filter(playbin.element(), &effects)?;
        *current_effects = effects;
        Ok(())
    }

    fn set_current_gain_db(&self, gain_db: f64) -> Result<(), PlaybackError> {
        let playbin = self.playbin.lock().unwrap_or_else(PoisonError::into_inner);
        set_playbin_track_gain(playbin.element(), gain_db)?;
        // The watchdog starts a hung start over from the record, so it carries
        // the gain as it is now.
        if let Some(record) = self
            .swap
            .start_record
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_mut()
        {
            record.gain_db = gain_db;
        }
        Ok(())
    }

    fn set_spectrum_enabled(&self, enabled: bool) -> Result<(), PlaybackError> {
        let playbin = self.playbin.lock().unwrap_or_else(PoisonError::into_inner);
        set_playbin_spectrum_messages(playbin.element(), enabled)?;
        drop(playbin);
        if let Some(incoming) = self
            .incoming
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            set_playbin_spectrum_messages(incoming.element(), enabled)?;
        }
        self.spectrum_enabled.store(enabled, Ordering::SeqCst);
        Ok(())
    }

    fn stop(&self) -> Result<(), PlaybackError> {
        self.stop_playback()
    }

    /// Pre-feeds the next track's URI into the gapless slot that the
    /// `about-to-finish` handler (installed in `build_playbin`) consumes for a
    /// seamless hand-off. `None` — or a path that fails to resolve to a URI —
    /// clears the slot (falling back to the ordinary `TrackFinished`-driven
    /// advance); an invalid path is logged, never panicked on. "Last write
    /// wins": the frontend re-feeds on every queue change.
    ///
    /// With a CUE track on either side only one pairing is prepared: the next
    /// track of the same file, starting where the playing one ends, is armed
    /// for the boundary probe (see `player/segment.rs`). Every other pairing
    /// goes into no slot at all — the playing track finishes and the frontend
    /// starts the next one with `play`, a short gap the plan accepts.
    fn set_next(&self, item: Option<PlaybackItem<'_>>) {
        let resolved = match item {
            Some(item) => match path_to_uri(item.path) {
                Ok(uri) => Some(QueuedTrack {
                    uri,
                    gain_db: item.gain_db,
                    segment: item.segment,
                }),
                Err(error) => {
                    tracing::warn!(%error, path = item.path, "set_next: invalid path; clearing gapless slot");
                    None
                }
            },
            None => None,
        };
        if resolved
            .as_ref()
            .is_some_and(|queued| self.refresh_in_flight_gain(queued))
        {
            return;
        }
        let prefeed = self.segments.route_next(resolved.as_ref());
        let slot = resolved.filter(|_| prefeed);
        *self.next_uri.lock().unwrap_or_else(PoisonError::into_inner) = slot;
    }

    /// Stores the transition mode + crossfade overlap. Takes effect on the next
    /// track boundary without a restart: the `about-to-finish` handler reads the
    /// mode to decide whether to gaplessly swap (Gapless only — see
    /// `gapless.rs`), and the position ticker reads mode + seconds to decide
    /// whether/when to start an overlapping crossfade (see `crossfade.rs`).
    /// Switching *away* from Crossfade does not interrupt an already-running
    /// fade — that would cut audio mid-blend; it simply won't start new ones.
    fn set_transition(&self, mode: TrackTransition, crossfade_seconds: u8) {
        *self
            .transition
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = (mode, crossfade_seconds);
    }

    fn current_generation(&self) -> StreamGeneration {
        StreamGeneration::from(self.stream_generation.load(Ordering::SeqCst))
    }
}

#[cfg(test)]
mod tests;
