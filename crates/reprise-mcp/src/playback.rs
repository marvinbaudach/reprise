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
}
