use gstreamer as gst;
use gstreamer::prelude::*;
use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum PlayerError {
    #[error("GStreamer: {0}")]
    Gst(String),
    #[error("invalid path: {0}")]
    BadPath(String),
}

pub fn path_to_uri(path: &str) -> Result<String, PlayerError> {
    if !path.starts_with('/') {
        return Err(PlayerError::BadPath(path.into()));
    }
    gst::glib::filename_to_uri(path, None)
        .map(|u| u.to_string())
        .map_err(|e| PlayerError::BadPath(e.to_string()))
}

/// Coarse playback state, mirrored from the underlying GStreamer pipeline state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Playing,
    Paused,
    Stopped,
}

/// Events the player reports asynchronously, from the GStreamer bus watch and
/// the position ticker. The UI layer subscribes to these via the callback
/// passed to `Player::new`.
#[derive(Debug, Clone)]
pub enum PlayerEvent {
    StateChanged(PlaybackState),
    Position { position_ms: i64, duration_ms: i64 },
    TrackFinished,
    Error(String),
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
}

/// Builds a fresh `playbin3` element with the `REPRISE_AUDIO_SINK` override
/// applied, if set. Extracted out of `Player::new` so `Player::rebuild_
/// playbin` (the wedged-pipeline recovery — see `Player::play`'s doc
/// comment) can build an identically-configured replacement element.
fn build_playbin() -> Result<gst::Element, PlayerError> {
    let playbin = gst::ElementFactory::make("playbin3")
        .build()
        .map_err(|e| PlayerError::Gst(e.to_string()))?;

    if let Ok(sink_name) = std::env::var(AUDIO_SINK_ENV_VAR) {
        let sink = gst::ElementFactory::make(&sink_name)
            .build()
            .map_err(|e| PlayerError::Gst(e.to_string()))?;
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
) -> Result<gst::bus::BusWatchGuard, PlayerError> {
    let bus = playbin
        .bus()
        .ok_or_else(|| PlayerError::Gst("no bus".into()))?;
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
    .map_err(|e| PlayerError::Gst(e.to_string()))
}

impl Player {
    /// Creates a new player and starts its background bus watch and position
    /// ticker. `on_event` is invoked (from either the GLib bus-watch context
    /// or the ticker thread) whenever a `PlayerEvent` occurs; it is wrapped in
    /// an `Arc` so both can share it.
    pub fn new(
        on_event: Box<dyn Fn(PlayerEvent) + Send + Sync + 'static>,
    ) -> Result<Self, PlayerError> {
        gst::init().map_err(|e| PlayerError::Gst(e.to_string()))?;

        let on_event: Arc<dyn Fn(PlayerEvent) + Send + Sync> = Arc::from(on_event);

        let playbin = build_playbin()?;
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
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                guard.clone()
            };
            if element.current_state() == gst::State::Playing {
                let position_ms = element
                    .query_position::<gst::ClockTime>()
                    .map(|t| t.mseconds() as i64)
                    .unwrap_or(0);
                let duration_ms = element
                    .query_duration::<gst::ClockTime>()
                    .map(|t| t.mseconds() as i64)
                    .unwrap_or(0);
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
        })
    }

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
    pub fn play(&self, path: &str) -> Result<(), PlayerError> {
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

    /// One playback attempt on the *current* pipeline: `Null` → set the new
    /// URI → `Playing`. Shared by `play`'s first attempt and its post-
    /// rebuild retry (DRY) — see `play`'s doc comment.
    fn try_play(&self, uri: &str) -> Result<(), PlayerError> {
        let playbin = self
            .playbin
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        playbin
            .set_state(gst::State::Null)
            .map_err(|e| PlayerError::Gst(e.to_string()))?;
        playbin.set_property("uri", uri);
        playbin
            .set_state(gst::State::Playing)
            .map_err(|e| PlayerError::Gst(e.to_string()))?;
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
    fn rebuild_playbin(&self) -> Result<(), PlayerError> {
        let new_playbin = build_playbin()?;
        let new_watch = attach_bus_watch(&new_playbin, self.on_event.clone())?;

        let mut playbin = self
            .playbin
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *playbin = new_playbin;
        drop(playbin);

        // The old guard's `Drop` removes the old (now-discarded) element's
        // bus watch — exactly what should happen when it's replaced.
        let _old_watch = self.bus_watch.replace(new_watch);
        Ok(())
    }

    pub fn toggle_pause(&self) -> Result<PlaybackState, PlayerError> {
        let playbin = self
            .playbin
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let next = match playbin.current_state() {
            gst::State::Playing => (gst::State::Paused, PlaybackState::Paused),
            _ => (gst::State::Playing, PlaybackState::Playing),
        };
        playbin
            .set_state(next.0)
            .map_err(|e| PlayerError::Gst(e.to_string()))?;
        drop(playbin);
        (self.on_event)(PlayerEvent::StateChanged(next.1));
        Ok(next.1)
    }

    pub fn seek_to(&self, position_ms: i64) -> Result<(), PlayerError> {
        let playbin = self
            .playbin
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        playbin
            .seek_simple(
                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                gst::ClockTime::from_mseconds(position_ms.max(0) as u64),
            )
            .map_err(|e| PlayerError::Gst(e.to_string()))
    }

    pub fn set_volume(&self, volume: f64) {
        let playbin = self
            .playbin
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        playbin.set_property("volume", volume.clamp(0.0, 1.0));
    }

    pub fn stop(&self) -> Result<(), PlayerError> {
        let playbin = self
            .playbin
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        playbin
            .set_state(gst::State::Null)
            .map_err(|e| PlayerError::Gst(e.to_string()))?;
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

    /// End-to-end proof that the callback plumbing actually reaches the UI
    /// layer: `play()` must emit `StateChanged(Playing)` and `stop()` must
    /// emit `StateChanged(Stopped)`. Runs headless via `REPRISE_AUDIO_SINK`
    /// (fakesink), which GStreamer supports without a real audio device.
    /// This is the only test in the crate that touches process environment,
    /// so there is no cross-test race to guard against.
    #[test]
    fn play_and_stop_emit_state_changed_events() {
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

    /// Stage 2 Task 5 regression test for the wedged-pipeline recovery (see
    /// `Player::play`'s doc comment): a failed `play()` against a
    /// nonexistent file must not take down subsequent, valid `play()` calls
    /// on the same `Player` instance.
    #[test]
    fn play_recovers_after_a_failed_attempt() {
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
}
