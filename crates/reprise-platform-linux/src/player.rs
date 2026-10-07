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
    attach_bus_watch, build_playbin, configure_download_buffering, path_to_uri,
    validated_playback_uri,
};
use segment::{SegmentGate, SegmentHandle};

pub(crate) mod segment;

/// Default playback volume before the user ever moves the slider — full scale,
/// matching `playbin3`'s own `volume` property default. Also the value the
/// crossfade ramp restores the promoted pipeline to.
const DEFAULT_VOLUME: f64 = 1.0;

const POSITION_TICK_INTERVAL: Duration = Duration::from_millis(500);

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
    /// The live GStreamer pipeline. `Arc<Mutex<_>>`, not a plain field: the
    /// position-ticker thread (spawned in `new`) needs to read whichever
    /// element is *current* on every tick, and `play`'s wedged-pipeline
    /// recovery (see its doc comment) can swap this out for a freshly built
    /// element after a failure. A plain field (or a value captured once by
    /// the ticker thread at spawn time, as this used to be) would go stale
    /// the moment a rebuild happened, silently freezing position ticks for
    /// the rest of the process's life.
    playbin: Arc<Mutex<gst::Element>>,
    on_event: Arc<dyn Fn(PlayerEvent) + Send + Sync + 'static>,
    /// Held for its `Drop` side effect (removes the bus watch) and replaced
    /// wholesale by `rebuild_playbin` and by the crossfade promotion, since a
    /// `BusWatchGuard` is tied to the specific `Bus` (and thus element) it was
    /// created for — a rebuilt/promoted playbin needs its own fresh watch, not
    /// the old element's. `Arc<Mutex<_>>` rather than `RefCell` because the
    /// crossfade ramp thread (see `crossfade.rs`) swaps it from off-thread when
    /// it promotes the incoming pipeline.
    bus_watch: Arc<Mutex<gst::bus::BusWatchGuard>>,
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
    /// Incremented for every GStreamer stream start so a gapless handoff
    /// cannot inherit the previous track's FFT, gravity, or sensitivity
    /// state. Purely internal to CAVA — not `stream_generation` below.
    cava_stream_generation: Arc<AtomicU64>,
    /// Bumped synchronously on every `play`/`play_uri` (`try_play`) or
    /// gapless/crossfade hand-off — the `PlaybackBackend` "Stream
    /// generations" contract. Stamped onto events as a `StreamEvent` for
    /// `new_with_generation` consumers.
    stream_generation: Arc<AtomicU64>,
    /// The stretch of the file a CUE track covers, while one plays — see
    /// `player/segment.rs`. Shared with the position ticker.
    segments: SegmentHandle,
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
    /// bump sites (`try_play`, the gapless hand-off, the crossfade promotion).
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

        let playbin = build_playbin(
            &effects.lock().unwrap_or_else(PoisonError::into_inner),
            next_uri.clone(),
            handoff_pending.clone(),
            transition.clone(),
            stream_generation.clone(),
            pending_gain.clone(),
            segments.clone(),
        )?;
        let bus_watch = attach_bus_watch(
            &playbin,
            on_event.clone(),
            handoff_pending.clone(),
            crossfading.clone(),
            spectrum_enabled.clone(),
            cava_stream_generation.clone(),
            segments.clone(),
        )?;
        let playbin = Arc::new(Mutex::new(playbin));
        let bus_watch = Arc::new(Mutex::new(bus_watch));

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
                    guard.clone()
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
            bus_watch,
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
            cava_stream_generation,
            stream_generation,
            segments,
        })
    }

    /// Aborts any in-flight crossfade cleanly: bumps the fade generation (so the
    /// ramp thread notices it is superseded and terminates without touching the
    /// now-discarded elements), clears the `crossfading` guard, silences and
    /// drops the incoming secondary pipeline, and restores the (outgoing)
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
            let _ = secondary.set_state(gst::State::Null);
        }
        let user_volume = *self
            .user_volume
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.playbin
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .set_property("volume", user_volume);
    }

    /// Clears both transition slots: the gapless pre-fed successor *and* any
    /// in-flight crossfade. Called on every hard restart (`play`,
    /// `rebuild_playbin`) — a pre-fed/overlapping successor is only valid
    /// relative to the track it was queued behind.
    fn reset_transition(&self) {
        self.abort_crossfade();
        self.reset_gapless();
        self.segments.reset();
    }

    /// Clears the gapless slot and the handoff flag. Called on every manual
    /// jump (`play`) and on `rebuild_playbin`: a pre-fed successor is only valid
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

    /// One playback attempt on the *current* pipeline: `Null` → set the new
    /// URI → `Playing`. Shared by `play`'s first attempt and its post-
    /// rebuild retry (DRY) — see `play`'s doc comment. A CUE track's
    /// `segment` is prerolled and sought to before `Playing` (see
    /// `segment::start_segment`).
    ///
    /// Bumps `stream_generation` only once `Playing` is entered (a failed
    /// attempt never emits an event, nothing to mislabel), still under the
    /// `playbin` lock so no event — `StateChanged` below included — sees stale.
    fn try_play(
        &self,
        uri: &str,
        live: bool,
        gain_db: f64,
        segment: Option<(i64, i64)>,
    ) -> Result<(), PlaybackError> {
        let playbin = self
            .playbin
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        playbin
            .set_state(gst::State::Null)
            .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))?;
        configure_download_buffering(&playbin, uri, live)?;
        set_playbin_track_gain(&playbin, gain_db)?;
        playbin.set_property("uri", uri);
        if let Some(segment) = segment {
            segment::start_segment(&playbin, &self.segments, uri, segment)?;
        }
        playbin
            .set_state(gst::State::Playing)
            .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))?;
        self.stream_generation.fetch_add(1, Ordering::SeqCst);
        drop(playbin);
        (self.on_event)(PlayerEvent::StateChanged(PlaybackState::Playing));
        Ok(())
    }

    fn play_resolved_uri(
        &self,
        uri: &str,
        source: &str,
        live: bool,
        gain_db: f64,
        segment: Option<(i64, i64)>,
    ) -> Result<(), PlaybackError> {
        // A manual jump invalidates every gapless/crossfade transition. This
        // applies equally to local paths and external media.
        self.reset_transition();
        match self.try_play(uri, live, gain_db, segment) {
            Ok(()) => Ok(()),
            Err(error) => {
                tracing::warn!(
                    %error,
                    source,
                    "playback failed; rebuilding pipeline and retrying once"
                );
                self.rebuild_playbin()?;
                self.try_play(uri, live, gain_db, segment)
            }
        }
    }

    /// Discards the current playbin element and replaces it with a freshly
    /// built one (see `play`'s `## Wedged-pipeline recovery` doc section),
    /// re-attaching an equivalent bus watch. The position ticker picks up
    /// the replacement automatically on its next tick (it reads through the
    /// same `Arc<Mutex<_>>`, not a stale clone — see the `playbin` field's
    /// doc comment).
    fn rebuild_playbin(&self) -> Result<(), PlaybackError> {
        let effects = self
            .effects
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        // A rebuild is a hard restart: any pre-fed successor and any in-flight
        // crossfade are now stale.
        self.reset_transition();
        let new_playbin = build_playbin(
            &effects,
            self.next_uri.clone(),
            self.handoff_pending.clone(),
            self.transition.clone(),
            self.stream_generation.clone(),
            self.pending_gain.clone(),
            self.segments.clone(),
        )?;
        set_playbin_spectrum_messages(&new_playbin, self.spectrum_enabled.load(Ordering::SeqCst))?;
        let new_watch = attach_bus_watch(
            &new_playbin,
            self.on_event.clone(),
            self.handoff_pending.clone(),
            self.crossfading.clone(),
            self.spectrum_enabled.clone(),
            self.cava_stream_generation.clone(),
            self.segments.clone(),
        )?;

        let mut playbin = self.playbin.lock().unwrap_or_else(PoisonError::into_inner);

        // Transition the old playbin to Null before discarding it to ensure
        // proper resource cleanup (decoders, file descriptors, buffers).
        // Ignore transition failures since the element is already broken.
        if let Err(error) = playbin.set_state(gst::State::Null) {
            tracing::debug!(%error, "old playbin refused Null transition during rebuild (already broken; dropping anyway)");
        }

        *playbin = new_playbin;
        drop(playbin);

        // Swapping the guard drops the old (now-discarded) element's bus watch —
        // exactly what should happen when it's replaced.
        *self
            .bus_watch
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = new_watch;
        Ok(())
    }
}

impl PlaybackBackend for Player {
    /// Starts playback of `path`. On a `set_state(Playing)` failure, retries
    /// exactly once against a freshly rebuilt pipeline (see `rebuild_
    /// playbin`) before giving up — see the module's `## Wedged-pipeline
    /// recovery` note below for why. Transparent to callers either way: they
    /// only ever see the final `Ok(())`/`Err`.
    ///
    /// ## Wedged-pipeline recovery (Stage 2 Task 5)
    ///
    /// Empirically (headless `fakesink` E2E, diagnosing a deleted-file
    /// scenario): once a `playbin3` instance's `set_state(Playing)` fails
    /// even once — for *any* reason, including simply naming a file that no
    /// longer exists — every subsequent `Playing` transition on that *same*
    /// instance also fails, even for a completely valid file afterwards.
    /// Ruled out as the cause, each individually confirmed not to matter:
    /// a full `Null` cycle before retrying, blocking on `get_state` until
    /// fully settled, draining any pending bus messages first, and pumping
    /// the `glib::MainContext` for 300ms before retrying. The only reliable
    /// recovery found is discarding the element and building a new one.
    /// Rebuilding a pipeline obviously can't make a genuinely missing file
    /// exist, so a real "file not found" still correctly fails the retry
    /// too (`skip_after_failure` in `ui::player_controller` still marks it
    /// missing and moves on) — but a merely *wedged* pipeline self-heals
    /// here instead of silently taking every subsequent queue track down
    /// with it, which is the actual fault-tolerance property Task 5 exists
    /// to guarantee (a deleted file must never crash *or dead-end* the app).
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
        let playbin = self
            .playbin
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let next = match playbin.current_state() {
            gst::State::Playing => (gst::State::Paused, PlaybackState::Paused),
            _ => (gst::State::Playing, PlaybackState::Playing),
        };
        playbin
            .set_state(next.0)
            .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))?;
        drop(playbin);
        (self.on_event)(PlayerEvent::StateChanged(next.1));
        Ok(next.1)
    }

    /// Seeks within the current track. A CUE track's `position_ms` is its
    /// own: the seek lands sample-accurately inside the track's stretch of
    /// the file. A whole file seeks to the nearest key unit, as it always has.
    fn seek_to(&self, position_ms: i64) -> Result<(), PlaybackError> {
        // A seek within the current track abandons any crossfade that may have
        // begun in its tail (the overlap position no longer applies); the
        // gapless slot stays valid — we are still on the same track. `next_uri`
        // is left intact so an already-in-progress fade that consumed it does
        // not silently lose the successor for the rest of the track.
        self.abort_crossfade();
        let (flags, target_ms) = match self.segments.seek_target_ms(position_ms) {
            Some(file_position_ms) => (
                gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
                file_position_ms,
            ),
            None => (
                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                position_ms,
            ),
        };
        let playbin = self.playbin.lock().unwrap_or_else(PoisonError::into_inner);
        playbin
            .seek_simple(
                flags,
                gst::ClockTime::from_mseconds(target_ms.max(0) as u64),
            )
            .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))
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
        update_existing_audio_filter(&playbin, &effects)?;
        *current_effects = effects;
        Ok(())
    }

    fn set_current_gain_db(&self, gain_db: f64) -> Result<(), PlaybackError> {
        let playbin = self.playbin.lock().unwrap_or_else(PoisonError::into_inner);
        set_playbin_track_gain(&playbin, gain_db)
    }

    fn set_spectrum_enabled(&self, enabled: bool) -> Result<(), PlaybackError> {
        let playbin = self.playbin.lock().unwrap_or_else(PoisonError::into_inner);
        set_playbin_spectrum_messages(&playbin, enabled)?;
        drop(playbin);
        if let Some(incoming) = self
            .incoming
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            set_playbin_spectrum_messages(incoming, enabled)?;
        }
        self.spectrum_enabled.store(enabled, Ordering::SeqCst);
        Ok(())
    }

    fn stop(&self) -> Result<(), PlaybackError> {
        // A full stop tears down everything: abort any crossfade (silencing and
        // dropping the incoming pipeline) and clear the gapless slot.
        self.reset_transition();
        let playbin = self.playbin.lock().unwrap_or_else(PoisonError::into_inner);
        playbin
            .set_state(gst::State::Null)
            .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))?;
        drop(playbin);
        (self.on_event)(PlayerEvent::StateChanged(PlaybackState::Stopped));
        Ok(())
    }

    /// Pre-feeds the next track's URI into the gapless slot that the
    /// `about-to-finish` handler (installed in `build_playbin`) consumes for a
    /// seamless hand-off. `None` — or a path that fails to resolve to a URI —
    /// clears the slot (falling back to the ordinary `TrackFinished`-driven
    /// advance); an invalid path is logged, never panicked on. "Last write
    /// wins": the frontend re-feeds on every queue change.
    fn set_next(&self, item: Option<PlaybackItem<'_>>) {
        let resolved = match item {
            Some(item) => match path_to_uri(item.path) {
                Ok(uri) => Some(QueuedTrack {
                    uri,
                    gain_db: item.gain_db,
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
        // The next track of the same CUE file, starting where this one ends,
        // plays through inside the file: the boundary probe hands over to it,
        // so it never goes through the URI slot.
        let armed = match (&resolved, item) {
            (Some(queued), Some(item)) => {
                self.segments
                    .arm_next(&queued.uri, item.segment, queued.gain_db)
            }
            _ => {
                self.segments.disarm();
                false
            }
        };
        let slot = resolved.filter(|_| !armed);
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

impl Player {
    /// A re-fed `next` that names the track whose hand-off is already under way
    /// (a live ReplayGain change re-feeds the unchanged next track) must update
    /// the gain that hand-off will apply: the gapless gain pending for the next
    /// stream start, or the gain of the pre-built crossfade secondary. Returns
    /// `true` when it did, so the slot is not refilled with a track that is
    /// already playing.
    fn refresh_in_flight_gain(&self, queued: &QueuedTrack) -> bool {
        let playbin = self
            .playbin
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        {
            let mut pending = self
                .pending_gain
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if pending.is_some() && playbin_uri(&playbin).as_deref() == Some(&queued.uri) {
                *pending = Some(queued.gain_db);
                return true;
            }
        }
        let incoming = self
            .incoming
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let Some(secondary) = incoming else {
            return false;
        };
        if playbin_uri(&secondary).as_deref() != Some(&queued.uri) {
            return false;
        }
        if let Err(error) = set_playbin_track_gain(&secondary, queued.gain_db) {
            tracing::warn!(%error, "could not refresh the crossfade secondary's gain");
        }
        true
    }
}

fn playbin_uri(playbin: &gst::Element) -> Option<String> {
    playbin.property::<Option<String>>("uri")
}

#[cfg(test)]
mod tests;
