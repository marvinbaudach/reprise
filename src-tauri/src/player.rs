use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::Mutex;
use tauri::Emitter;

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

pub struct Player {
    playbin: gst::Element,
    // Must be held: dropping the guard removes the bus watch again.
    _bus_watch: gst::bus::BusWatchGuard,
}

impl Player {
    pub fn new(app: tauri::AppHandle) -> Result<Self, PlayerError> {
        gst::init().map_err(|e| PlayerError::Gst(e.to_string()))?;
        let playbin = gst::ElementFactory::make("playbin3")
            .build()
            .map_err(|e| PlayerError::Gst(e.to_string()))?;

        // Bus watch: report end-of-stream and errors to the frontend.
        let bus = playbin.bus().ok_or_else(|| PlayerError::Gst("no bus".into()))?;
        let app_bus = app.clone();
        let bus_watch = bus
            .add_watch(move |_, msg| {
                use gst::MessageView;
                match msg.view() {
                    MessageView::Eos(_) => {
                        tracing::debug!("playback reached end-of-stream");
                        let _ = app_bus.emit("player:track-finished", serde_json::json!({}));
                    }
                    MessageView::Error(e) => {
                        tracing::error!(error = %e.error(), debug = ?e.debug(), "GStreamer bus error");
                        let _ = app_bus.emit(
                            "player:state",
                            serde_json::json!({ "state": "stopped", "error": e.error().to_string() }),
                        );
                    }
                    _ => {}
                }
                gst::glib::ControlFlow::Continue
            })
            .map_err(|e| PlayerError::Gst(e.to_string()))?;

        // Position ticker: emit position + duration every 500 ms while playing.
        let tick_playbin = playbin.clone();
        let app_tick = app.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
            if tick_playbin.current_state() == gst::State::Playing {
                let pos = tick_playbin
                    .query_position::<gst::ClockTime>()
                    .map(|t| t.mseconds() as i64)
                    .unwrap_or(0);
                let dur = tick_playbin
                    .query_duration::<gst::ClockTime>()
                    .map(|t| t.mseconds() as i64)
                    .unwrap_or(0);
                let _ = app_tick.emit(
                    "player:position",
                    serde_json::json!({ "positionMs": pos, "durationMs": dur }),
                );
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

    pub fn toggle_pause(&self) -> Result<&'static str, PlayerError> {
        let next = match self.playbin.current_state() {
            gst::State::Playing => (gst::State::Paused, "paused"),
            _ => (gst::State::Playing, "playing"),
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

/// The player is created lazily on first `play_track` because it needs the
/// `AppHandle` (to emit events), which is only available once Tauri has
/// finished setting up its command context.
pub struct PlayerState(pub Mutex<Option<Player>>);

#[tauri::command]
pub fn play_track(
    app: tauri::AppHandle,
    ps: tauri::State<PlayerState>,
    path: String,
) -> Result<(), String> {
    let mut guard = ps.0.lock().map_err(|e| {
        tracing::error!("failed to lock player state: {e}");
        e.to_string()
    })?;
    if guard.is_none() {
        *guard = Some(Player::new(app.clone()).map_err(|e| {
            tracing::error!("failed to initialize player: {e}");
            e.to_string()
        })?);
    }
    let player = guard
        .as_ref()
        .ok_or_else(|| "player not initialized".to_string())?;
    player.play(&path).map_err(|e| {
        tracing::error!(path = %path, "play_track failed: {e}");
        e.to_string()
    })?;
    tracing::info!(path = %path, "starting playback");
    let _ = app.emit("player:state", serde_json::json!({ "state": "playing" }));
    let _ = app.emit("player:track-changed", serde_json::json!({ "path": path }));
    Ok(())
}

#[tauri::command]
pub fn toggle_pause(app: tauri::AppHandle, ps: tauri::State<PlayerState>) -> Result<(), String> {
    let guard = ps.0.lock().map_err(|e| {
        tracing::error!("failed to lock player state: {e}");
        e.to_string()
    })?;
    if let Some(p) = guard.as_ref() {
        let state = p.toggle_pause().map_err(|e| {
            tracing::error!("toggle_pause failed: {e}");
            e.to_string()
        })?;
        tracing::debug!(state, "playback state toggled");
        let _ = app.emit("player:state", serde_json::json!({ "state": state }));
    }
    Ok(())
}

#[tauri::command]
pub fn seek_to(ps: tauri::State<PlayerState>, position_ms: i64) -> Result<(), String> {
    let guard = ps.0.lock().map_err(|e| {
        tracing::error!("failed to lock player state: {e}");
        e.to_string()
    })?;
    if let Some(p) = guard.as_ref() {
        p.seek_to(position_ms).map_err(|e| {
            tracing::error!(position_ms, "seek_to failed: {e}");
            e.to_string()
        })?;
    }
    Ok(())
}

#[tauri::command]
pub fn set_volume(ps: tauri::State<PlayerState>, volume: f64) -> Result<(), String> {
    let guard = ps.0.lock().map_err(|e| {
        tracing::error!("failed to lock player state: {e}");
        e.to_string()
    })?;
    if let Some(p) = guard.as_ref() {
        p.set_volume(volume);
    }
    Ok(())
}

#[tauri::command]
pub fn stop(app: tauri::AppHandle, ps: tauri::State<PlayerState>) -> Result<(), String> {
    let guard = ps.0.lock().map_err(|e| {
        tracing::error!("failed to lock player state: {e}");
        e.to_string()
    })?;
    if let Some(p) = guard.as_ref() {
        p.stop().map_err(|e| {
            tracing::error!("stop failed: {e}");
            e.to_string()
        })?;
        tracing::debug!("playback stopped");
        let _ = app.emit("player:state", serde_json::json!({ "state": "stopped" }));
    }
    Ok(())
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
