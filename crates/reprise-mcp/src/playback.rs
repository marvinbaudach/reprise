//! Session-bus client for controlling the running Reprise app (feature `mpris`).
//!
//! Transport goes to `org.mpris.MediaPlayer2.Player`; targeted play goes to the
//! Reprise-specific `org.reprise.Player1.PlayTrackIds`. Mirrors reprise-cli's
//! playback command (Beschluss 3: `zbus` DIRECTLY, no `reprise-platform-linux`
//! dependency — the one sanctioned exception to the "workspace surfaces depend
//! on reprise-core only" rule). Every call here is blocking and MUST be run
//! inside `tokio::task::spawn_blocking` by the caller.
//!
//! `transport`/`play_track_ids` are wired to the `music_playback_control`/
//! `music_play` tools in `server.rs`. The D-Bus round trip itself needs a live
//! app and is deliberately NOT unit-tested (same boundary reprise-cli draws);
//! only `TransportAction`/`from_str` have a test in this file.

use std::collections::HashMap;

use zbus::zvariant::{OwnedObjectPath, OwnedValue};

use crate::dto::{PlaybackStateDto, SetPlaybackParams};

/// The app's MPRIS well-known name (mirrors `reprise-platform-linux`'s server).
const BUS_NAME: &str = "org.mpris.MediaPlayer2.reprise";
/// The standard MPRIS object path and player interface.
const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";
/// The Reprise-specific interface carrying `PlayTrackIds`.
const REPRISE_INTERFACE: &str = "org.reprise.Player1";

/// A transport action recognised by the `music_playback_control` tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportAction {
    Play,
    Pause,
    Stop,
    Next,
    Previous,
}

impl TransportAction {
    /// Parses a tool-facing verb into an action; unrecognised strings are
    /// `None` so the caller can report a clear invalid-input error.
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "play" => Some(Self::Play),
            "pause" => Some(Self::Pause),
            "stop" => Some(Self::Stop),
            "next" => Some(Self::Next),
            "previous" => Some(Self::Previous),
            _ => None,
        }
    }

    /// The MPRIS `org.mpris.MediaPlayer2.Player` method this action invokes.
    fn method(self) -> &'static str {
        match self {
            Self::Play => "Play",
            Self::Pause => "Pause",
            Self::Stop => "Stop",
            Self::Next => "Next",
            Self::Previous => "Previous",
        }
    }
}

/// A playback client failure. Mapped to a tool outcome by
/// `error::playback_outcome`, exactly like [`crate::data::DataError`] is
/// mapped by `error::into_tool_outcome`.
#[derive(Debug)]
pub enum PlaybackError {
    /// No MPRIS player is registered under our bus name — the app is not
    /// running.
    NoPlayer,
    /// A genuine D-Bus/session-bus fault, carrying `zbus::Error`'s message.
    Bus(String),
}

/// One validated live-setting change for the running player.
#[derive(Debug, Clone, PartialEq)]
pub enum PlaybackSetting {
    Volume(f64),
    SeekMicros(i64),
    Shuffle(bool),
    Repeat(&'static str),
}

impl PlaybackSetting {
    pub fn from_params(params: &SetPlaybackParams) -> Result<Self, String> {
        match params.action.as_str() {
            "set_volume" => {
                let value = params
                    .volume
                    .ok_or_else(|| "set_volume requires volume".to_owned())?;
                if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                    return Err("volume must be between 0 and 1".to_owned());
                }
                Ok(Self::Volume(value))
            }
            "seek" => {
                let seconds = params
                    .offset_seconds
                    .ok_or_else(|| "seek requires offset_seconds".to_owned())?;
                let micros = seconds * 1_000_000.0;
                if !micros.is_finite() || micros.abs() > i64::MAX as f64 {
                    return Err("offset_seconds is outside the supported range".to_owned());
                }
                Ok(Self::SeekMicros(micros.round() as i64))
            }
            "set_shuffle" => params
                .enabled
                .map(Self::Shuffle)
                .ok_or_else(|| "set_shuffle requires enabled".to_owned()),
            "set_repeat" => match params.repeat.as_deref() {
                Some("off") => Ok(Self::Repeat("None")),
                Some("all") => Ok(Self::Repeat("Playlist")),
                Some("one") => Ok(Self::Repeat("Track")),
                Some(_) => Err("repeat must be off, all, or one".to_owned()),
                None => Err("set_repeat requires repeat".to_owned()),
            },
            other => Err(format!("unknown action '{other}'")),
        }
    }

    fn summary(&self) -> String {
        match self {
            Self::Volume(value) => format!("Playback volume set to {value:.2}"),
            Self::SeekMicros(offset) => {
                format!(
                    "Playback seeked by {:.3} second(s)",
                    *offset as f64 / 1_000_000.0
                )
            }
            Self::Shuffle(enabled) => format!("Playback shuffle set to {enabled}"),
            Self::Repeat(value) => format!("Playback repeat set to {}", repeat_from_mpris(value)),
        }
    }
}

/// D-Bus error names that mean no MPRIS player is registered under our name —
/// i.e. the Reprise app is not running. Anything else is a genuine fault.
/// Mirrors `reprise-cli`'s `commands::playback::is_absent_player` exactly.
fn is_absent_player(error_name: &str) -> bool {
    matches!(
        error_name,
        "org.freedesktop.DBus.Error.ServiceUnknown" | "org.freedesktop.DBus.Error.NameHasNoOwner"
    )
}

/// Opens the session bus and a proxy to the app on the given interface. A
/// missing session bus is a `Bus` error; an absent player is classified by
/// `map_zbus_error`, mirroring `reprise-cli`'s `connect`.
fn connect(interface: &'static str) -> Result<zbus::blocking::Proxy<'static>, PlaybackError> {
    let connection = zbus::blocking::Connection::session()
        .map_err(|error| PlaybackError::Bus(format!("no D-Bus session bus available: {error}")))?;
    zbus::blocking::Proxy::new(&connection, BUS_NAME, OBJECT_PATH, interface)
        .map_err(|error| map_zbus_error(&error))
}

/// Maps a zbus error to a playback error, recognising the "no player" case.
/// Mirrors `reprise-cli`'s `commands::playback::map_zbus_error` exactly (same
/// `MethodError` destructuring, same absent-player classification).
fn map_zbus_error(error: &zbus::Error) -> PlaybackError {
    if let zbus::Error::MethodError(name, _, _) = error {
        if is_absent_player(name.as_str()) {
            return PlaybackError::NoPlayer;
        }
    }
    PlaybackError::Bus(error.to_string())
}

fn map_fdo_error(error: &zbus::fdo::Error) -> PlaybackError {
    match error {
        zbus::fdo::Error::ServiceUnknown(_) | zbus::fdo::Error::NameHasNoOwner(_) => {
            PlaybackError::NoPlayer
        }
        zbus::fdo::Error::ZBus(error) => map_zbus_error(error),
        _ => PlaybackError::Bus(error.to_string()),
    }
}

/// Sends a transport action (play/pause/stop/next/previous) to the app's
/// MPRIS player.
pub fn transport(action: TransportAction) -> Result<(), PlaybackError> {
    let proxy = connect(PLAYER_INTERFACE)?;
    let _reply: () = proxy
        .call(action.method(), &())
        .map_err(|error| map_zbus_error(&error))?;
    Ok(())
}

/// Starts playback at a specific set of track ids via the Reprise-specific
/// `org.reprise.Player1.PlayTrackIds` method.
pub fn play_track_ids(ids: Vec<i64>) -> Result<(), PlaybackError> {
    let proxy = connect(REPRISE_INTERFACE)?;
    let _reply: () = proxy
        .call("PlayTrackIds", &(ids,))
        .map_err(|error| map_zbus_error(&error))?;
    Ok(())
}

/// Reads the running player's live, path-free MPRIS state.
pub fn state() -> Result<PlaybackStateDto, PlaybackError> {
    let proxy = connect(PLAYER_INTERFACE)?;
    let status: String = get_property(&proxy, "PlaybackStatus")?;
    let metadata: HashMap<String, OwnedValue> = get_property(&proxy, "Metadata")?;
    let position_micros: i64 = get_property(&proxy, "Position")?;
    let volume: f64 = get_property(&proxy, "Volume")?;
    let shuffle: bool = get_property(&proxy, "Shuffle")?;
    let repeat: String = get_property(&proxy, "LoopStatus")?;

    Ok(PlaybackStateDto {
        status: status.to_ascii_lowercase(),
        track_id: metadata_track_id(&metadata),
        title: metadata_string(&metadata, "xesam:title"),
        artist: metadata_artists(&metadata),
        album: metadata_string(&metadata, "xesam:album"),
        duration_ms: metadata_i64(&metadata, "mpris:length") / 1_000,
        position_ms: position_micros / 1_000,
        volume,
        shuffle,
        repeat: repeat_from_mpris(&repeat).to_owned(),
    })
}

/// Applies one validated MPRIS setting to the running player.
pub fn set(setting: PlaybackSetting) -> Result<String, PlaybackError> {
    let proxy = connect(PLAYER_INTERFACE)?;
    match &setting {
        PlaybackSetting::Volume(value) => proxy
            .set_property("Volume", *value)
            .map_err(|error| map_fdo_error(&error))?,
        PlaybackSetting::SeekMicros(offset) => {
            let _: () = proxy
                .call("Seek", &(*offset,))
                .map_err(|error| map_zbus_error(&error))?;
        }
        PlaybackSetting::Shuffle(enabled) => proxy
            .set_property("Shuffle", *enabled)
            .map_err(|error| map_fdo_error(&error))?,
        PlaybackSetting::Repeat(value) => proxy
            .set_property("LoopStatus", *value)
            .map_err(|error| map_fdo_error(&error))?,
    }
    Ok(setting.summary())
}

fn get_property<T>(proxy: &zbus::blocking::Proxy<'_>, name: &str) -> Result<T, PlaybackError>
where
    T: TryFrom<OwnedValue>,
    T::Error: Into<zbus::Error>,
{
    proxy
        .get_property(name)
        .map_err(|error| map_zbus_error(&error))
}

fn metadata_string(metadata: &HashMap<String, OwnedValue>, key: &str) -> String {
    metadata
        .get(key)
        .and_then(|value| String::try_from(value.clone()).ok())
        .unwrap_or_default()
}

fn metadata_artists(metadata: &HashMap<String, OwnedValue>) -> String {
    metadata
        .get("xesam:artist")
        .and_then(|value| Vec::<String>::try_from(value.clone()).ok())
        .map(|artists| artists.join(", "))
        .unwrap_or_default()
}

fn metadata_i64(metadata: &HashMap<String, OwnedValue>, key: &str) -> i64 {
    metadata
        .get(key)
        .and_then(|value| i64::try_from(value.clone()).ok())
        .unwrap_or_default()
}

fn metadata_track_id(metadata: &HashMap<String, OwnedValue>) -> Option<i64> {
    let path = metadata
        .get("mpris:trackid")
        .and_then(|value| OwnedObjectPath::try_from(value.clone()).ok())?;
    path.as_str().rsplit('/').next()?.parse().ok()
}

fn repeat_from_mpris(value: &str) -> &'static str {
    match value {
        "Playlist" => "all",
        "Track" => "one",
        _ => "off",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_action_parses_the_five_verbs_and_rejects_others() {
        for (s, a) in [
            ("play", TransportAction::Play),
            ("pause", TransportAction::Pause),
            ("stop", TransportAction::Stop),
            ("next", TransportAction::Next),
            ("previous", TransportAction::Previous),
        ] {
            assert_eq!(TransportAction::from_str(s), Some(a));
        }
        assert_eq!(TransportAction::from_str("rewind"), None);
    }

    #[test]
    fn setting_params_are_validated_and_mapped_to_mpris_values() {
        let params = SetPlaybackParams {
            action: "set_repeat".to_owned(),
            volume: None,
            offset_seconds: None,
            enabled: None,
            repeat: Some("all".to_owned()),
        };
        assert_eq!(
            PlaybackSetting::from_params(&params).unwrap(),
            PlaybackSetting::Repeat("Playlist")
        );
    }
}
