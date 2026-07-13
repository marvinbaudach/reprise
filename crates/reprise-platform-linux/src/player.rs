use gstreamer as gst;
use gstreamer::prelude::*;
use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reprise_core::playback::{
    AudioEffects, PlaybackBackend, PlaybackError, PlaybackState, PlayerEvent,
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
}

/// Builds a fresh `playbin3` element with the `REPRISE_AUDIO_SINK` override
/// applied, if set. Extracted out of `Player::new` so `Player::rebuild_
/// playbin` (the wedged-pipeline recovery — see `Player::play`'s doc
/// comment) can build an identically-configured replacement element.
fn build_audio_filter(effects: &AudioEffects) -> Result<Option<gst::Element>, PlaybackError> {
    use reprise_core::library::settings::ReplayGainMode;
    if !effects.equalizer_enabled && effects.replay_gain == ReplayGainMode::Off {
        return Ok(None);
    }
    let bin = gst::Bin::new();
    let first = gst::ElementFactory::make("audioconvert")
        .build()
        .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))?;
    let mut elements = vec![first];
    if effects.equalizer_enabled {
        let equalizer = gst::ElementFactory::make("equalizer-10bands")
            .name("reprise-equalizer")
            .build()
            .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))?;
        for (index, value) in effects.equalizer_bands.iter().enumerate() {
            equalizer.set_property(&format!("band{index}"), value.clamp(-12.0, 12.0));
        }
        elements.push(equalizer);
    }
    if effects.replay_gain != ReplayGainMode::Off {
        let replaygain = gst::ElementFactory::make("rgvolume")
            .name("reprise-replaygain")
            .build()
            .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))?;
        replaygain.set_property("album-mode", effects.replay_gain == ReplayGainMode::Album);
        elements.push(replaygain);
    }
    elements.push(
        gst::ElementFactory::make("audioconvert")
            .build()
            .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))?,
    );
    bin.add_many(elements.iter().collect::<Vec<_>>())
        .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))?;
    gst::Element::link_many(elements.iter().collect::<Vec<_>>())
        .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))?;
    let sink = elements[0]
        .static_pad("sink")
        .ok_or_else(|| PlaybackError::Backend("GStreamer: filter has no sink pad".into()))?;
    let src = elements
        .last()
        .and_then(|e| e.static_pad("src"))
        .ok_or_else(|| PlaybackError::Backend("GStreamer: filter has no src pad".into()))?;
    bin.add_pad(
        &gst::GhostPad::with_target(&sink)
            .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))?,
    )
    .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))?;
    bin.add_pad(
        &gst::GhostPad::with_target(&src)
            .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))?,
    )
    .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))?;
    Ok(Some(bin.upcast()))
}

fn apply_audio_filter(playbin: &gst::Element, effects: &AudioEffects) -> Result<(), PlaybackError> {
    let filter = build_audio_filter(effects)?;
    playbin.set_property("audio-filter", filter.as_ref());
    Ok(())
}

fn same_filter_topology(current: &AudioEffects, next: &AudioEffects) -> bool {
    use reprise_core::library::settings::ReplayGainMode;
    current.equalizer_enabled == next.equalizer_enabled
        && (current.replay_gain != ReplayGainMode::Off) == (next.replay_gain != ReplayGainMode::Off)
}

/// Updates properties on the existing filter bin when no elements need to be
/// added or removed. This is safe while Playing and avoids a Null → Playing
/// round trip for every intermediate equalizer-slider value.
fn update_existing_audio_filter(
    playbin: &gst::Element,
    current: &AudioEffects,
    next: &AudioEffects,
) -> bool {
    use reprise_core::library::settings::ReplayGainMode;
    if !same_filter_topology(current, next) {
        return false;
    }
    if !next.equalizer_enabled && next.replay_gain == ReplayGainMode::Off {
        return true;
    }
    let Some(filter) = playbin.property::<Option<gst::Element>>("audio-filter") else {
        return false;
    };
    let Ok(bin) = filter.downcast::<gst::Bin>() else {
        return false;
    };
    if next.equalizer_enabled {
        let Some(equalizer) = bin.by_name("reprise-equalizer") else {
            return false;
        };
        for (index, value) in next.equalizer_bands.iter().enumerate() {
            equalizer.set_property(&format!("band{index}"), value.clamp(-12.0, 12.0));
        }
    }
    if next.replay_gain != ReplayGainMode::Off {
        let Some(replaygain) = bin.by_name("reprise-replaygain") else {
            return false;
        };
        replaygain.set_property("album-mode", next.replay_gain == ReplayGainMode::Album);
    }
    true
}

fn requested_state(element: &gst::Element) -> gst::State {
    let (_, current, pending) = element.state(gst::ClockTime::ZERO);
    if pending == gst::State::VoidPending {
        current
    } else {
        pending
    }
}

fn restore_requested_state(
    playbin: &gst::Element,
    state: gst::State,
    position: Option<gst::ClockTime>,
) -> Result<(), PlaybackError> {
    if state == gst::State::Null {
        return Ok(());
    }
    playbin
        .set_state(state)
        .map_err(|error| PlaybackError::Backend(format!("GStreamer: {error}")))?;
    if let Some(position) = position {
        let _ = playbin.seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT, position);
    }
    Ok(())
}

fn replace_audio_filter(
    playbin: &gst::Element,
    effects: &AudioEffects,
    apply: impl FnOnce(&gst::Element, &AudioEffects) -> Result<(), PlaybackError>,
) -> Result<(), PlaybackError> {
    let state = requested_state(playbin);
    let position = playbin.query_position::<gst::ClockTime>();
    playbin
        .set_state(gst::State::Null)
        .map_err(|error| PlaybackError::Backend(format!("GStreamer: {error}")))?;
    let apply_result = apply(playbin, effects);
    let restore_result = restore_requested_state(playbin, state, position);
    match (apply_result, restore_result) {
        (Err(error), Err(restore_error)) => {
            tracing::warn!(%restore_error, "could not restore playback after filter failure");
            Err(error)
        }
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

fn build_playbin(effects: &AudioEffects) -> Result<gst::Element, PlaybackError> {
    let playbin = gst::ElementFactory::make("playbin3")
        .build()
        .map_err(|e| PlaybackError::Backend(format!("GStreamer: {e}")))?;
    apply_audio_filter(&playbin, effects)?;

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
        let playbin = build_playbin(
            &effects
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )?;
        let bus_watch = attach_bus_watch(&playbin, on_event.clone())?;
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
        })
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
        let new_playbin = build_playbin(&effects)?;
        let new_watch = attach_bus_watch(&new_playbin, self.on_event.clone())?;

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_to_uri_encodes_special_chars() {
        let uri = path_to_uri("/home/marvin/Music/Björk/Jóga (Live).flac").unwrap();
        assert!(uri.starts_with("file:///home/marvin/Music/"));
        assert!(uri.contains("J%C3%B3ga%20(Live).flac"));
        assert!(path_to_uri("relativ/pfad.mp3").is_err());
    }

    #[test]
    fn audio_filter_contains_configured_equalizer_and_replaygain() {
        gst::init().unwrap();
        let effects = AudioEffects {
            equalizer_enabled: true,
            equalizer_bands: [3.0; 10],
            replay_gain: reprise_core::library::settings::ReplayGainMode::Album,
        };
        let filter = build_audio_filter(&effects).unwrap().unwrap();
        let bin = filter.downcast::<gst::Bin>().unwrap();
        assert!(bin.by_name("reprise-equalizer").is_some());
        let replaygain = bin.by_name("reprise-replaygain").unwrap();
        assert!(replaygain.property::<bool>("album-mode"));
    }

    /// Guards every test in this module that sets `AUDIO_SINK_ENV_VAR`:
    /// `std::env::set_var`/`remove_var` affect the whole process, and
    /// `cargo test` runs tests in this module concurrently by default. Two
    /// such tests running at once can interleave — one test's `remove_var`
    /// landing between the other's `set_var` and `build_playbin`'s env
    /// read — so that `build_playbin` sees no override, builds a *real*
    /// audio sink, and plays `sine.flac` audibly on the developer's desktop
    /// (or simply fails to find `fakesink`'s paced-sync behavior headless,
    /// flaking the test). Each test that touches this env var must acquire
    /// this lock for its *entire* duration, from the `set_var` through the
    /// matching `remove_var`, so no two such tests ever overlap.
    ///
    /// Poisoned-recovery, not `.unwrap()`: if an earlier test in this lock
    /// panicked while holding it, the lock is poisoned but the environment
    /// variable was still cleaned up correctly enough for the next test to
    /// proceed — refusing to run every subsequent audio-sink test over one
    /// unrelated panic would be worse than the poisoning itself.
    static AUDIO_SINK_TEST_LOCK: Mutex<()> = Mutex::new(());

    /// End-to-end proof that the callback plumbing actually reaches the UI
    /// layer: `play()` must emit `StateChanged(Playing)` and `stop()` must
    /// emit `StateChanged(Stopped)`. Runs headless via `REPRISE_AUDIO_SINK`
    /// (fakesink), which GStreamer supports without a real audio device.
    /// This and `play_recovers_after_a_failed_attempt` are the only tests in
    /// the crate that touch process environment; both hold
    /// `AUDIO_SINK_TEST_LOCK` for their full duration to prevent the
    /// cross-test race documented on that lock.
    #[test]
    fn play_and_stop_emit_state_changed_events() {
        let _guard = AUDIO_SINK_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::env::set_var(AUDIO_SINK_ENV_VAR, "fakesink");

        let (tx, rx) = std::sync::mpsc::channel::<PlayerEvent>();
        let player = Player::new(Box::new(move |event| {
            let _ = tx.send(event);
        }))
        .unwrap();

        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sine.flac");
        player.play(path).unwrap();

        let playing_timeout = Duration::from_secs(5);
        let event = rx
            .recv_timeout(playing_timeout)
            .expect("expected a StateChanged(Playing) event within timeout");
        assert!(matches!(
            event,
            PlayerEvent::StateChanged(PlaybackState::Playing)
        ));

        player.stop().unwrap();
        let event = rx
            .recv_timeout(playing_timeout)
            .expect("expected a StateChanged(Stopped) event within timeout");
        assert!(matches!(
            event,
            PlayerEvent::StateChanged(PlaybackState::Stopped)
        ));

        std::env::remove_var(AUDIO_SINK_ENV_VAR);
    }

    #[test]
    fn live_audio_effect_change_preserves_a_playable_pipeline() {
        let _guard = AUDIO_SINK_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::env::set_var(AUDIO_SINK_ENV_VAR, "fakesink");
        let player = Player::new(Box::new(|_| {})).unwrap();
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sine.flac");
        player.play(path).unwrap();
        let effects = AudioEffects {
            equalizer_enabled: true,
            equalizer_bands: [2.0; 10],
            replay_gain: reprise_core::library::settings::ReplayGainMode::Track,
        };
        player.set_audio_effects(effects.clone()).unwrap();
        assert_eq!(
            *player
                .effects
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            effects
        );

        let filter_before = player
            .playbin
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .property::<Option<gst::Element>>("audio-filter")
            .unwrap();
        let adjusted = AudioEffects {
            equalizer_enabled: true,
            equalizer_bands: [5.0; 10],
            replay_gain: reprise_core::library::settings::ReplayGainMode::Album,
        };
        player.set_audio_effects(adjusted).unwrap();
        let playbin = player
            .playbin
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let filter_after = playbin
            .property::<Option<gst::Element>>("audio-filter")
            .unwrap();
        assert_eq!(filter_after, filter_before);
        assert_eq!(requested_state(&playbin), gst::State::Playing);
        let bin = filter_after.downcast::<gst::Bin>().unwrap();
        assert_eq!(
            bin.by_name("reprise-equalizer")
                .unwrap()
                .property::<f64>("band0"),
            5.0
        );
        assert!(bin
            .by_name("reprise-replaygain")
            .unwrap()
            .property::<bool>("album-mode"));
        drop(playbin);
        player.stop().unwrap();
        std::env::remove_var(AUDIO_SINK_ENV_VAR);
    }

    #[test]
    fn failed_filter_replacement_restores_requested_playback_state() {
        let _guard = AUDIO_SINK_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::env::set_var(AUDIO_SINK_ENV_VAR, "fakesink");
        let player = Player::new(Box::new(|_| {})).unwrap();
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sine.flac");
        player.play(path).unwrap();
        let playbin = player
            .playbin
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let result = replace_audio_filter(&playbin, &AudioEffects::default(), |_, _| {
            Err(PlaybackError::Backend("injected filter failure".into()))
        });

        assert!(result.is_err());
        assert_eq!(requested_state(&playbin), gst::State::Playing);
        drop(playbin);
        player.stop().unwrap();
        std::env::remove_var(AUDIO_SINK_ENV_VAR);
    }

    /// Stage 2 Task 5 regression test for the wedged-pipeline recovery (see
    /// `Player::play`'s doc comment): a failed `play()` against a
    /// nonexistent file must not take down subsequent, valid `play()` calls
    /// on the same `Player` instance. Holds `AUDIO_SINK_TEST_LOCK` for its
    /// full duration — see that lock's doc comment for why.
    #[test]
    fn play_recovers_after_a_failed_attempt() {
        let _guard = AUDIO_SINK_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::env::set_var(AUDIO_SINK_ENV_VAR, "fakesink");

        let (tx, rx) = std::sync::mpsc::channel::<PlayerEvent>();
        let player = Player::new(Box::new(move |event| {
            let _ = tx.send(event);
        }))
        .unwrap();

        let missing_path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/does-not-exist.flac"
        );
        assert!(
            player.play(missing_path).is_err(),
            "playing a nonexistent file must fail, not panic"
        );

        let valid_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sine.flac");
        assert!(
            player.play(valid_path).is_ok(),
            "a valid file must still play successfully after a prior failure \
             on the same Player — this is the wedged-pipeline recovery this \
             test guards against regressing"
        );

        let playing_timeout = Duration::from_secs(5);
        let event = rx
            .recv_timeout(playing_timeout)
            .expect("expected a StateChanged(Playing) event within timeout");
        assert!(matches!(
            event,
            PlayerEvent::StateChanged(PlaybackState::Playing)
        ));

        std::env::remove_var(AUDIO_SINK_ENV_VAR);
    }

    /// Portability seam (refactor Task 5): drives play/stop through
    /// `Box<dyn PlaybackBackend>` — the exact shape the controller holds — to
    /// pin that the trait surface alone is enough to operate the backend.
    #[test]
    fn playback_backend_trait_object_drives_play_and_stop() {
        let _guard = AUDIO_SINK_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::env::set_var(AUDIO_SINK_ENV_VAR, "fakesink");

        let (tx, rx) = std::sync::mpsc::channel::<PlayerEvent>();
        let player = Player::new(Box::new(move |event| {
            let _ = tx.send(event);
        }))
        .unwrap();

        let backend: Box<dyn PlaybackBackend> = Box::new(player);

        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sine.flac");
        backend.play(path).unwrap();

        let playing_timeout = Duration::from_secs(5);
        let event = rx
            .recv_timeout(playing_timeout)
            .expect("expected a StateChanged(Playing) event within timeout");
        assert!(matches!(
            event,
            PlayerEvent::StateChanged(PlaybackState::Playing)
        ));

        backend.stop().unwrap();
        let event = rx
            .recv_timeout(playing_timeout)
            .expect("expected a StateChanged(Stopped) event within timeout");
        assert!(matches!(
            event,
            PlayerEvent::StateChanged(PlaybackState::Stopped)
        ));

        std::env::remove_var(AUDIO_SINK_ENV_VAR);
    }
}
