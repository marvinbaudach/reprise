use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::Arc;
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
    playbin: gst::Element,
    // Must be held: dropping the guard removes the bus watch again.
    _bus_watch: gst::bus::BusWatchGuard,
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
        let playbin = gst::ElementFactory::make("playbin3")
            .build()
            .map_err(|e| PlayerError::Gst(e.to_string()))?;

        if let Ok(sink_name) = std::env::var(AUDIO_SINK_ENV_VAR) {
            let sink = gst::ElementFactory::make(&sink_name)
                .build()
                .map_err(|e| PlayerError::Gst(e.to_string()))?;
            tracing::info!(sink = %sink_name, "REPRISE_AUDIO_SINK override active");
            playbin.set_property("audio-sink", &sink);
        }

        let on_event: Arc<dyn Fn(PlayerEvent) + Send + Sync> = Arc::from(on_event);

        // Bus watch: report end-of-stream and errors to the callback.
        let bus = playbin.bus().ok_or_else(|| PlayerError::Gst("no bus".into()))?;
        let bus_event = on_event.clone();
        let bus_watch = bus
            .add_watch(move |_, msg| {
                use gst::MessageView;
                match msg.view() {
                    MessageView::Eos(_) => {
                        tracing::debug!("playback reached end-of-stream");
                        (*bus_event)(PlayerEvent::TrackFinished);
                    }
                    MessageView::Error(e) => {
                        tracing::error!(error = %e.error(), debug = ?e.debug(), "GStreamer bus error");
                        (*bus_event)(PlayerEvent::Error(e.error().to_string()));
                    }
                    _ => {}
                }
                gst::glib::ControlFlow::Continue
            })
            .map_err(|e| PlayerError::Gst(e.to_string()))?;

        // Position ticker: report position + duration every 500 ms while playing.
        let tick_playbin = playbin.clone();
        let tick_event = on_event.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(POSITION_TICK_INTERVAL);
            if tick_playbin.current_state() == gst::State::Playing {
                let position_ms = tick_playbin
                    .query_position::<gst::ClockTime>()
                    .map(|t| t.mseconds() as i64)
                    .unwrap_or(0);
                let duration_ms = tick_playbin
                    .query_duration::<gst::ClockTime>()
                    .map(|t| t.mseconds() as i64)
                    .unwrap_or(0);
                (*tick_event)(PlayerEvent::Position { position_ms, duration_ms });
            }
        });

        Ok(Self {
            playbin,
            _bus_watch: bus_watch,
        })
    }

    pub fn play(&self, path: &str) -> Result<(), PlayerError> {
        let uri = path_to_uri(path)?;
        self.playbin
            .set_state(gst::State::Null)
            .map_err(|e| PlayerError::Gst(e.to_string()))?;
        self.playbin.set_property("uri", &uri);
        self.playbin
            .set_state(gst::State::Playing)
            .map_err(|e| PlayerError::Gst(e.to_string()))?;
        Ok(())
    }

    pub fn toggle_pause(&self) -> Result<PlaybackState, PlayerError> {
        let next = match self.playbin.current_state() {
            gst::State::Playing => (gst::State::Paused, PlaybackState::Paused),
            _ => (gst::State::Playing, PlaybackState::Playing),
        };
        self.playbin
            .set_state(next.0)
            .map_err(|e| PlayerError::Gst(e.to_string()))?;
        Ok(next.1)
    }

    pub fn seek_to(&self, position_ms: i64) -> Result<(), PlayerError> {
        self.playbin
            .seek_simple(
                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                gst::ClockTime::from_mseconds(position_ms.max(0) as u64),
            )
            .map_err(|e| PlayerError::Gst(e.to_string()))
    }

    pub fn set_volume(&self, volume: f64) {
        self.playbin.set_property("volume", volume.clamp(0.0, 1.0));
    }

    pub fn stop(&self) -> Result<(), PlayerError> {
        self.playbin
            .set_state(gst::State::Null)
            .map(|_| ())
            .map_err(|e| PlayerError::Gst(e.to_string()))
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
}
