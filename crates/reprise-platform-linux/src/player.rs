use gstreamer as gst;
use gstreamer::prelude::*;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reprise_core::playback::{
    AudioEffects, PlaybackBackend, PlaybackError, PlaybackState, PlayerEvent,
};

use crate::gapless::{connect_about_to_finish, note_stream_start, HandoffFlag, NextUri};
use crate::player_effects::{
    apply_audio_filter, replace_audio_filter, update_existing_audio_filter,
};

pub fn path_to_uri(path: &str) -> Result<String, PlaybackError> {
    if !path.starts_with('/') {
        return Err(PlaybackError::BadPath(path.into()));
    }
    gst::glib::filename_to_uri(path, None)
        .map(|u| u.to_string())
        .map_err(|e| PlaybackError::BadPath(e.to_string()))
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
    /// wholesale by `rebuild_playbin`, since a `BusWatchGuard` is tied to the
    /// specific `Bus` (and thus element) it was created for — a rebuilt
    /// playbin needs its own fresh watch, not the old element's.
    bus_watch: RefCell<gst::bus::BusWatchGuard>,
    effects: Arc<Mutex<AudioEffects>>,
    /// Gapless slot: the URI pre-fed via `set_next`, consumed by the
    /// `about-to-finish` handler installed in `build_playbin`. Shared with that
    /// handler (a GStreamer streaming thread) — see `gapless.rs`.
    next_uri: NextUri,
    /// Set by the `about-to-finish` handler when it hands off a pre-fed URI,
    /// cleared by the bus watch's `StreamStart` handling — together they
    /// distinguish a gapless handoff (emit `AdvancedToNext`) from an ordinary
    /// first-track `StreamStart` (emit nothing). See `gapless.rs`.
    handoff_pending: HandoffFlag,
}

/// Builds a fresh `playbin3` element with the `REPRISE_AUDIO_SINK` override
/// applied, if set. Extracted out of `Player::new` so `Player::rebuild_
/// playbin` (the wedged-pipeline recovery — see `Player::play`'s doc
/// comment) can build an identically-configured replacement element.
fn build_playbin(
    effects: &AudioEffects,
    next_uri: NextUri,
    handoff_pending: HandoffFlag,
) -> Result<gst::Element, PlaybackError> {
    let playbin = gst::ElementFactory::make("playbin3")
        .build()
        .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))?;
    apply_audio_filter(&playbin, effects)?;

    // Gapless handoff: consume any pre-fed URI on `about-to-finish` without a
    // pipeline restart. Installed here so `rebuild_playbin` re-arms it on the
    // replacement element for free.
    connect_about_to_finish(&playbin, next_uri, handoff_pending);

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
/// `on_event`. Extracted out of `Player::new` so `Player::rebuild_playbin`
/// can re-attach an identically-behaving watch to a replacement element (a
/// `BusWatchGuard`/`Bus` is tied to the specific element it came from, so a
/// rebuilt playbin needs its own watch rather than reusing the old one).
fn attach_bus_watch(
    playbin: &gst::Element,
    on_event: Arc<dyn Fn(PlayerEvent) + Send + Sync>,
    handoff_pending: HandoffFlag,
) -> Result<gst::bus::BusWatchGuard, PlaybackError> {
    let bus = playbin
        .bus()
        .ok_or_else(|| PlaybackError::Backend("GStreamer: no bus".into()))?;
    bus.add_watch(move |_, msg| {
        use gst::MessageView;
        match msg.view() {
            MessageView::Eos(_) => {
                tracing::debug!("playback reached end-of-stream");
                (*on_event)(PlayerEvent::TrackFinished);
            }
            MessageView::StreamStart(_) => {
                // Fires on every stream start; only a gapless handoff (flagged
                // by the `about-to-finish` handler) turns into `AdvancedToNext`.
                note_stream_start(&handoff_pending, on_event.as_ref());
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
        let playbin = build_playbin(
            &effects
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            next_uri.clone(),
            handoff_pending.clone(),
        )?;
        let bus_watch = attach_bus_watch(&playbin, on_event.clone(), handoff_pending.clone())?;
        let playbin = Arc::new(Mutex::new(playbin));

        // Position ticker: report position + duration every 500 ms while
        // playing. Reads whichever element is current at each tick (see the
        // `playbin` field's doc comment), holding the mutex only long enough
        // to clone the `gst::Element` handle out (a cheap refcount bump) —
        // the actual state/position queries run outside the lock.
        let tick_playbin = playbin.clone();
        let tick_event = on_event.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(POSITION_TICK_INTERVAL);
            let element = {
                let guard = tick_playbin
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                guard.clone()
            };
            if element.current_state() == gst::State::Playing {
                let position_ms = element
                    .query_position::<gst::ClockTime>()
                    .map_or(0, |t| t.mseconds() as i64);
                let duration_ms = element
                    .query_duration::<gst::ClockTime>()
                    .map_or(0, |t| t.mseconds() as i64);
                (*tick_event)(PlayerEvent::Position {
                    position_ms,
                    duration_ms,
                });
            }
        });

        Ok(Self {
            playbin,
            on_event,
            bus_watch: RefCell::new(bus_watch),
            effects,
            next_uri,
            handoff_pending,
        })
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
        // A rebuild is a hard restart: any pre-fed successor is now stale.
        self.reset_gapless();
        let new_playbin = build_playbin(
            &effects,
            self.next_uri.clone(),
            self.handoff_pending.clone(),
        )?;
        let new_watch = attach_bus_watch(
            &new_playbin,
            self.on_event.clone(),
            self.handoff_pending.clone(),
        )?;

        let mut playbin = self
            .playbin
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // Transition the old playbin to Null before discarding it to ensure
        // proper resource cleanup (decoders, file descriptors, buffers).
        // Ignore transition failures since the element is already broken.
        if let Err(error) = playbin.set_state(gst::State::Null) {
            tracing::debug!(%error, "old playbin refused Null transition during rebuild (already broken; dropping anyway)");
        }

        *playbin = new_playbin;
        drop(playbin);

        // The old guard's `Drop` removes the old (now-discarded) element's
        // bus watch — exactly what should happen when it's replaced.
        let _old_watch = self.bus_watch.replace(new_watch);
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
        // A manual jump (new selection / skip) invalidates any gaplessly
        // pre-fed successor; the frontend re-feeds after this play() settles.
        self.reset_gapless();
        match self.try_play(&uri) {
            Ok(()) => Ok(()),
            Err(error) => {
                tracing::warn!(
                    %error,
                    path,
                    "playback failed; rebuilding pipeline and retrying once"
                );
                self.rebuild_playbin()?;
                self.try_play(&uri)
            }
        }
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
        let playbin = self
            .playbin
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        playbin
            .seek_simple(
                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                gst::ClockTime::from_mseconds(position_ms.max(0) as u64),
            )
            .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))
    }

    fn set_volume(&self, volume: f64) {
        let playbin = self
            .playbin
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        playbin.set_property("volume", volume.clamp(0.0, 1.0));
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
        *current_effects = effects;
        Ok(())
    }

    fn stop(&self) -> Result<(), PlaybackError> {
        let playbin = self
            .playbin
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
        *self
            .next_uri
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = resolved;
    }
}

#[cfg(test)]
mod tests;
