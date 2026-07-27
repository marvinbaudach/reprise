use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use reprise_core::library::settings::{TrackTransition, CROSSFADE_SECONDS_DEFAULT};
use reprise_core::playback::{
    AudioEffects, PlaybackBackend, PlaybackError, PlaybackState, PlayerEvent, SpectrumAnalyzer,
    SPECTRUM_ANALYSIS_BAND_COUNT,
};

use crate::crossfade::{CrossfadeEngine, IncomingSlot, Transition};
use crate::gapless::{connect_about_to_finish, note_stream_start, HandoffFlag, NextUri};
use crate::player_effects::{
    apply_audio_filter, replace_audio_filter, set_playbin_spectrum_messages,
    update_existing_audio_filter,
};

/// Extracts the raw per-band decibel magnitudes from a GStreamer `spectrum`
/// element message. The values are handed to a [`SpectrumAnalyzer`] (which owns
/// the cross-frame state) rather than turned into a frame here, so onset and
/// dynamics derivation stays in `reprise-core`.
pub(super) fn spectrum_decibels_from_structure(
    structure: &gst::StructureRef,
) -> Option<[f32; SPECTRUM_ANALYSIS_BAND_COUNT]> {
    if structure.name() != "spectrum" {
        return None;
    }
    let magnitudes = structure.get::<gst::List>("magnitude").ok()?;
    if magnitudes.len() != SPECTRUM_ANALYSIS_BAND_COUNT {
        return None;
    }
    let mut decibels = [0.0_f32; SPECTRUM_ANALYSIS_BAND_COUNT];
    for (slot, magnitude) in decibels.iter_mut().zip(magnitudes.iter()) {
        *slot = magnitude.get::<f32>().ok()?;
    }
    Some(decibels)
}

/// Default playback volume before the user ever moves the slider — full scale,
/// matching `playbin3`'s own `volume` property default. Also the value the
/// crossfade ramp restores the promoted pipeline to.
const DEFAULT_VOLUME: f64 = 1.0;

pub fn path_to_uri(path: &str) -> Result<String, PlaybackError> {
    if !path.starts_with('/') {
        return Err(PlaybackError::BadPath(path.into()));
    }
    gst::glib::filename_to_uri(path, None)
        .map(|u| u.to_string())
        .map_err(|e| PlaybackError::BadPath(e.to_string()))
}

pub fn validated_playback_uri(uri: &str) -> Result<String, PlaybackError> {
    let accepted = ["http://", "https://", "file://"]
        .iter()
        .any(|prefix| uri.starts_with(prefix) && uri.len() > prefix.len());
    if accepted {
        Ok(uri.to_owned())
    } else {
        Err(PlaybackError::BadPath(uri.into()))
    }
}

fn merge_stream_tags(
    previous: &(Option<String>, Option<String>),
    title: Option<String>,
    organization: Option<String>,
) -> Option<(Option<String>, Option<String>)> {
    let next = (
        title.or_else(|| previous.0.clone()),
        organization.or_else(|| previous.1.clone()),
    );
    (next != *previous).then_some(next)
}

/// Environment variable that, when set, overrides playbin's audio sink
/// element (e.g. `fakesink`). Used for headless verification in environments
/// without a real audio device.
const AUDIO_SINK_ENV_VAR: &str = "REPRISE_AUDIO_SINK";

const POSITION_TICK_INTERVAL: Duration = Duration::from_millis(500);

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
}

/// Builds a fresh `playbin3` element with the `REPRISE_AUDIO_SINK` override
/// applied, if set. Extracted out of `Player::new` so `Player::rebuild_
/// playbin` (the wedged-pipeline recovery — see `Player::play`'s doc comment)
/// and the crossfade ramp (which builds the identically-configured *secondary*
/// pipeline — see `crossfade.rs`) can build matching elements. `pub(crate)` for
/// that second caller.
pub(crate) fn build_playbin(
    effects: &AudioEffects,
    next_uri: NextUri,
    handoff_pending: HandoffFlag,
    transition: Transition,
) -> Result<gst::Element, PlaybackError> {
    let playbin = gst::ElementFactory::make("playbin3")
        .build()
        .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))?;
    apply_audio_filter(&playbin, effects)?;

    // Gapless handoff: consume any pre-fed URI on `about-to-finish` without a
    // pipeline restart (Gapless mode only — the handler no-ops in Crossfade/Off,
    // see `gapless.rs`). Installed here so `rebuild_playbin` and the crossfade
    // secondary re-arm it on the built element for free.
    connect_about_to_finish(&playbin, next_uri, handoff_pending, transition);

    if let Ok(sink_name) = std::env::var(AUDIO_SINK_ENV_VAR) {
        let sink = gst::ElementFactory::make(&sink_name)
            .build()
            .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))?;
        // Pace the override sink against the pipeline clock (if it has a
        // `sync` property): `fakesink` defaults to sync=false, which
        // would consume an entire track as fast as it decodes — EOS
        // after milliseconds, no position ticks — making headless runs
        // behave nothing like real playback. Real audio sinks default to
        // sync=true anyway, so this only affects test sinks.
        //
        // `find_property` only confirms a property named "sync" exists,
        // not that it's a `bool` — `set_property` panics on a type
        // mismatch. This path only runs for developer-chosen
        // `REPRISE_AUDIO_SINK` overrides (never in production), but an
        // exotic element with an unrelated "sync" property (wrong type)
        // must not be able to crash a headless dev run, so check the
        // property's declared type before setting it.
        let has_bool_sync = sink
            .find_property("sync")
            .is_some_and(|pspec| pspec.value_type() == gst::glib::Type::BOOL);
        if has_bool_sync {
            sink.set_property("sync", true);
        }
        tracing::info!(sink = %sink_name, "REPRISE_AUDIO_SINK override active");
        playbin.set_property("audio-sink", &sink);
    }

    Ok(playbin)
}

/// Attaches a bus watch to `playbin` that reports EOS/error messages via
/// `on_event`. Extracted out of `Player::new` so `Player::rebuild_playbin` and
/// the crossfade promotion (see `crossfade.rs`) can attach an identically-
/// behaving watch to a replacement/promoted element (a `BusWatchGuard`/`Bus` is
/// tied to the specific element it came from, so a rebuilt/promoted playbin
/// needs its own watch rather than reusing the old one). `pub(crate)` for the
/// crossfade caller.
///
/// `crossfading` gates the EOS→`TrackFinished` emission: while a crossfade is in
/// flight the *outgoing* pipeline (which still holds this watch until promotion)
/// naturally ends if the track is shorter than the fade overlap; that EOS must
/// not surface as a spurious `TrackFinished`, because the crossfade promotion is
/// the authoritative advance and emits `AdvancedToNext` itself.
pub(crate) fn attach_bus_watch(
    playbin: &gst::Element,
    on_event: Arc<dyn Fn(PlayerEvent) + Send + Sync>,
    handoff_pending: HandoffFlag,
    crossfading: Arc<AtomicBool>,
) -> Result<gst::bus::BusWatchGuard, PlaybackError> {
    let bus = playbin
        .bus()
        .ok_or_else(|| PlaybackError::Backend("GStreamer: no bus".into()))?;
    // Owns the cross-frame reactivity state for this pipeline's watch. Reset on
    // each stream start so one track's dynamics baseline never bleeds into the
    // next.
    let mut spectrum_analyzer = SpectrumAnalyzer::new();
    let mut stream_tags = (None::<String>, None::<String>);
    bus.add_watch(move |_, msg| {
        use gst::MessageView;
        match msg.view() {
            MessageView::Eos(_) => {
                if crossfading.load(Ordering::SeqCst) {
                    tracing::debug!(
                        "end-of-stream on the outgoing pipeline during a crossfade; \
                         suppressing TrackFinished (promotion drives the advance)"
                    );
                } else {
                    tracing::debug!("playback reached end-of-stream");
                    (*on_event)(PlayerEvent::TrackFinished);
                }
            }
            MessageView::StreamStart(_) => {
                // Fires on every stream start; only a gapless handoff (flagged
                // by the `about-to-finish` handler) turns into `AdvancedToNext`.
                spectrum_analyzer = SpectrumAnalyzer::new();
                stream_tags = (None, None);
                note_stream_start(&handoff_pending, on_event.as_ref());
            }
            MessageView::Tag(message) => {
                let tags = message.tags();
                let title = tags
                    .get::<gst::tags::Title>()
                    .map(|value| value.get().to_string());
                let organization = tags
                    .get::<gst::tags::Organization>()
                    .map(|value| value.get().to_string());
                if let Some(next) = merge_stream_tags(&stream_tags, title, organization) {
                    stream_tags = next.clone();
                    (*on_event)(PlayerEvent::StreamTags {
                        title: next.0,
                        organization: next.1,
                    });
                }
            }
            MessageView::Element(element) => {
                if let Some(structure) = element.structure() {
                    if let Some(decibels) = spectrum_decibels_from_structure(structure) {
                        (*on_event)(PlayerEvent::Spectrum(spectrum_analyzer.ingest(decibels)));
                    }
                }
            }
            MessageView::Error(e) => {
                tracing::error!(error = %e.error(), debug = ?e.debug(), "GStreamer bus error");
                (*on_event)(PlayerEvent::Error(e.error().to_string()));
            }
            _ => {}
        }
        gst::glib::ControlFlow::Continue
    })
    .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))
}

impl Player {
    /// Creates a new player and starts its background bus watch and position
    /// ticker. `on_event` is invoked (from either the GLib bus-watch context
    /// or the ticker thread) whenever a `PlayerEvent` occurs; it is wrapped in
    /// an `Arc` so both can share it.
    pub fn new(
        on_event: Box<dyn Fn(PlayerEvent) + Send + Sync + 'static>,
    ) -> Result<Self, PlaybackError> {
        gst::init().map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))?;

        let on_event: Arc<dyn Fn(PlayerEvent) + Send + Sync> = Arc::from(on_event);

        let effects = Arc::new(Mutex::new(AudioEffects::default()));
        let next_uri: NextUri = Arc::new(Mutex::new(None));
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

        let playbin = build_playbin(
            &effects.lock().unwrap_or_else(PoisonError::into_inner),
            next_uri.clone(),
            handoff_pending.clone(),
            transition.clone(),
        )?;
        let bus_watch = attach_bus_watch(
            &playbin,
            on_event.clone(),
            handoff_pending.clone(),
            crossfading.clone(),
        )?;
        let playbin = Arc::new(Mutex::new(playbin));
        let bus_watch = Arc::new(Mutex::new(bus_watch));

        let engine = CrossfadeEngine {
            playbin: playbin.clone(),
            bus_watch: bus_watch.clone(),
            on_event: on_event.clone(),
            effects: effects.clone(),
            next_uri: next_uri.clone(),
            handoff_pending: handoff_pending.clone(),
            transition: transition.clone(),
            crossfading: crossfading.clone(),
            user_volume: user_volume.clone(),
            generation: fade_generation.clone(),
            incoming: incoming.clone(),
            spectrum_enabled: spectrum_enabled.clone(),
        };

        // Position ticker: report position + duration every 500 ms while
        // playing, and — in Crossfade mode — start the overlapping fade once the
        // position enters the last `crossfade_seconds` window (see
        // `CrossfadeEngine::maybe_start`). Reads whichever element is current at
        // each tick (see the `playbin` field's doc comment), holding the mutex
        // only long enough to clone the `gst::Element` handle out (a cheap
        // refcount bump) — the actual state/position queries run outside the lock.
        let ticker = engine.clone();
        std::thread::spawn(move || loop {
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
                let duration_ms = element
                    .query_duration::<gst::ClockTime>()
                    .map_or(0, |t| t.mseconds() as i64);
                (ticker.on_event)(PlayerEvent::Position {
                    position_ms,
                    duration_ms,
                });
                ticker.maybe_start(position_ms, duration_ms);
            }
        });

        Ok(Self {
            playbin,
            on_event,
            bus_watch,
            effects,
            next_uri,
            handoff_pending,
            transition,
            crossfading,
            user_volume,
            fade_generation,
            incoming,
            spectrum_enabled,
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
        if let Some(secondary) = self
            .incoming
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
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
    }

    /// One playback attempt on the *current* pipeline: `Null` → set the new
    /// URI → `Playing`. Shared by `play`'s first attempt and its post-
    /// rebuild retry (DRY) — see `play`'s doc comment.
    fn try_play(&self, uri: &str) -> Result<(), PlaybackError> {
        let playbin = self
            .playbin
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        playbin
            .set_state(gst::State::Null)
            .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))?;
        playbin.set_property("uri", uri);
        playbin
            .set_state(gst::State::Playing)
            .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))?;
        drop(playbin);
        (self.on_event)(PlayerEvent::StateChanged(PlaybackState::Playing));
        Ok(())
    }

    fn play_resolved_uri(&self, uri: &str, source: &str) -> Result<(), PlaybackError> {
        // A manual jump invalidates every gapless/crossfade transition. This
        // applies equally to local paths and external media.
        self.reset_transition();
        match self.try_play(uri) {
            Ok(()) => Ok(()),
            Err(error) => {
                tracing::warn!(
                    %error,
                    source,
                    "playback failed; rebuilding pipeline and retrying once"
                );
                self.rebuild_playbin()?;
                self.try_play(uri)
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
        )?;
        set_playbin_spectrum_messages(&new_playbin, self.spectrum_enabled.load(Ordering::SeqCst))?;
        let new_watch = attach_bus_watch(
            &new_playbin,
            self.on_event.clone(),
            self.handoff_pending.clone(),
            self.crossfading.clone(),
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
    fn play(&self, path: &str) -> Result<(), PlaybackError> {
        let uri = path_to_uri(path)?;
        self.play_resolved_uri(&uri, path)
    }

    fn play_uri(&self, uri: &str) -> Result<(), PlaybackError> {
        let uri = validated_playback_uri(uri)?;
        self.play_resolved_uri(&uri, uri.as_str())
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

    fn seek_to(&self, position_ms: i64) -> Result<(), PlaybackError> {
        // A seek within the current track abandons any crossfade that may have
        // begun in its tail (the overlap position no longer applies); the
        // gapless slot stays valid — we are still on the same track. `next_uri`
        // is left intact so an already-in-progress fade that consumed it does
        // not silently lose the successor for the rest of the track.
        self.abort_crossfade();
        let playbin = self.playbin.lock().unwrap_or_else(PoisonError::into_inner);
        playbin
            .seek_simple(
                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                gst::ClockTime::from_mseconds(position_ms.max(0) as u64),
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
        if update_existing_audio_filter(&playbin, &current_effects, &effects) {
            *current_effects = effects;
            return Ok(());
        }
        replace_audio_filter(&playbin, &effects, apply_audio_filter)?;
        set_playbin_spectrum_messages(&playbin, self.spectrum_enabled.load(Ordering::SeqCst))?;
        *current_effects = effects;
        Ok(())
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
    fn set_next(&self, path: Option<&str>) {
        let resolved = match path {
            Some(path) => match path_to_uri(path) {
                Ok(uri) => Some(uri),
                Err(error) => {
                    tracing::warn!(%error, path, "set_next: invalid path; clearing gapless slot");
                    None
                }
            },
            None => None,
        };
        *self.next_uri.lock().unwrap_or_else(PoisonError::into_inner) = resolved;
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
}

#[cfg(test)]
mod tests;
