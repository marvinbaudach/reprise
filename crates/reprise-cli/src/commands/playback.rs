//! `playback` — a thin MPRIS transport client (cargo feature `mpris`, Linux).
//!
//! Beschluss 3: control the running app's playback over MPRIS using `zbus`
//! DIRECTLY — the one sanctioned exception to the "workspace surfaces depend on
//! reprise-core only" rule. There is deliberately no `reprise-platform-linux`
//! dependency: this is a standalone session-bus client that talks to the app's
//! existing `org.mpris.MediaPlayer2.reprise` player. It works only while the
//! app is running; otherwise every action reports a clear "no player" error.

use serde_json::json;

use crate::error::CliError;

use reprise_runtime_protocol::mpris::{is_absent_player, BUS_NAME, OBJECT_PATH, PLAYER_INTERFACE};

/// A transport action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    PlayPause,
    Next,
    Previous,
    Status,
}

impl Action {
    /// The MPRIS `org.mpris.MediaPlayer2.Player` method a control action invokes.
    /// `Status` reads properties instead, so it maps to no method.
    fn method(self) -> Option<&'static str> {
        match self {
            Self::PlayPause => Some("PlayPause"),
            Self::Next => Some("Next"),
            Self::Previous => Some("Previous"),
            Self::Status => None,
        }
    }
}

/// The clear, actionable message shown when no player is present.
fn no_player_error() -> CliError {
    CliError::Unavailable(format!(
        "no running Reprise player found on the session bus ({BUS_NAME}) — is the app running?"
    ))
}

/// Runs a transport action against the app's MPRIS player.
pub fn run(action: Action, json_output: bool) -> Result<(), CliError> {
    let proxy = connect()?;
    match action.method() {
        Some(method) => {
            invoke(&proxy, method)?;
            report_action(action, json_output);
            Ok(())
        }
        None => show_status(&proxy, json_output),
    }
}

/// Opens the session bus and a proxy to the app's player. A missing session bus
/// (e.g. a headless host) is an `Unavailable` error, not a panic.
fn connect() -> Result<zbus::Proxy<'static>, CliError> {
    let connection = zbus::block_on(zbus::Connection::session()).map_err(|error| {
        CliError::Unavailable(format!("no D-Bus session bus available: {error}"))
    })?;
    zbus::block_on(zbus::Proxy::new(
        &connection,
        BUS_NAME,
        OBJECT_PATH,
        PLAYER_INTERFACE,
    ))
    .map_err(|error| map_zbus_error(&error))
}

/// Calls a no-argument, no-return player method, mapping "no player" cleanly.
fn invoke(proxy: &zbus::Proxy<'static>, method: &str) -> Result<(), CliError> {
    let _reply: () =
        zbus::block_on(proxy.call(method, &())).map_err(|error| map_zbus_error(&error))?;
    Ok(())
}

/// Reads and prints the current playback status and track.
fn show_status(proxy: &zbus::Proxy<'static>, json_output: bool) -> Result<(), CliError> {
    let status: String = zbus::block_on(proxy.get_property("PlaybackStatus"))
        .map_err(|error| map_zbus_error(&error))?;
    let (title, artist) = read_now_playing(proxy);
    if json_output {
        crate::output::print_json(&json!({
            "status": status,
            "title": title,
            "artist": artist,
        }));
    } else {
        match (title, artist) {
            (Some(title), Some(artist)) => println!("{status}: {artist} - {title}"),
            (Some(title), None) => println!("{status}: {title}"),
            _ => println!("{status}"),
        }
    }
    Ok(())
}

/// Best-effort `xesam:title` / `xesam:artist` from the player's `Metadata`. Any
/// shape mismatch just yields `None` — status still prints without track info.
fn read_now_playing(proxy: &zbus::Proxy<'static>) -> (Option<String>, Option<String>) {
    use std::collections::HashMap;
    use zbus::OwnedVariant;

    let Ok(metadata) =
        zbus::block_on(proxy.get_property::<HashMap<String, OwnedVariant>>("Metadata"))
    else {
        return (None, None);
    };
    let title = metadata.get("xesam:title").and_then(owned_to_string);
    let artist = metadata.get("xesam:artist").and_then(owned_to_string);
    (title, artist)
}

/// Best-effort text from an MPRIS metadata value: `xesam:artist` is an array of
/// strings, `xesam:title` a plain string, so try both without letting one
/// failure short-circuit the other.
fn owned_to_string(value: &zbus::OwnedVariant) -> Option<String> {
    if let Ok(list) = value.try_clone().and_then(Vec::<String>::try_from) {
        return Some(list.join(", "));
    }
    value.try_clone().and_then(String::try_from).ok()
}

/// The D-Bus error name of a failed method call. zbus 6 reports the names that
/// `zbus::fdo::Error` defines (`ServiceUnknown`, `NameHasNoOwner`, `UnknownMethod`,
/// ...) as `Error::FDO` and only the rest as `Error::MethodError`, so both
/// variants have to be read.
fn dbus_error_name(error: &zbus::Error) -> Option<String> {
    match error {
        zbus::Error::MethodError(name, _, _) => Some(name.as_str().to_owned()),
        zbus::Error::FDO(error) => Some(zbus::DBusError::name(error.as_ref()).as_str().to_owned()),
        _ => None,
    }
}

/// Maps a zbus error to a CLI error, recognising the "no player" case.
fn map_zbus_error(error: &zbus::Error) -> CliError {
    if dbus_error_name(error).is_some_and(|name| is_absent_player(&name)) {
        return no_player_error();
    }
    CliError::Unavailable(format!("MPRIS request failed: {error}"))
}

fn report_action(action: Action, json_output: bool) {
    let name = match action {
        Action::PlayPause => "play-pause",
        Action::Next => "next",
        Action::Previous => "previous",
        Action::Status => "status",
    };
    if json_output {
        crate::output::print_json(&json!({ "action": name, "sent": true }));
    } else {
        println!("sent {name}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_actions_map_to_their_mpris_methods() {
        assert_eq!(Action::PlayPause.method(), Some("PlayPause"));
        assert_eq!(Action::Next.method(), Some("Next"));
        assert_eq!(Action::Previous.method(), Some("Previous"));
        // Status reads properties, not a method.
        assert_eq!(Action::Status.method(), None);
    }

    #[test]
    fn absent_player_error_names_are_recognised() {
        assert!(is_absent_player(
            "org.freedesktop.DBus.Error.ServiceUnknown"
        ));
        assert!(is_absent_player(
            "org.freedesktop.DBus.Error.NameHasNoOwner"
        ));
        // An unrelated D-Bus error is a genuine fault, not "no player".
        assert!(!is_absent_player("org.freedesktop.DBus.Error.AccessDenied"));
        assert!(!is_absent_player(""));
    }

    #[test]
    fn well_known_error_replies_are_recognised_by_their_wire_names() {
        // zbus 6 turns the names `zbus::fdo::Error` defines into `Error::FDO`,
        // so the absent-player check has to read them from there too.
        let absent = zbus::Error::FDO(Box::new(zbus::fdo::Error::ServiceUnknown(String::new())));
        assert_eq!(
            dbus_error_name(&absent).as_deref(),
            Some("org.freedesktop.DBus.Error.ServiceUnknown")
        );
        assert!(
            map_zbus_error(&absent).to_string().contains(BUS_NAME),
            "an absent player is reported as such"
        );
        let unowned = zbus::Error::FDO(Box::new(zbus::fdo::Error::NameHasNoOwner(String::new())));
        assert!(map_zbus_error(&unowned).to_string().contains(BUS_NAME));
        let other = zbus::Error::FDO(Box::new(zbus::fdo::Error::Failed(String::new())));
        assert_eq!(
            dbus_error_name(&other).as_deref(),
            Some("org.freedesktop.DBus.Error.Failed")
        );
        assert!(
            map_zbus_error(&other)
                .to_string()
                .contains("MPRIS request failed"),
            "a genuine fault is not swallowed as an absent player"
        );
        // An error that carries no D-Bus name has none to report.
        assert_eq!(dbus_error_name(&zbus::Error::Unsupported), None);
    }

    #[test]
    fn no_player_error_is_unavailable_and_names_the_bus() {
        let error = no_player_error();
        assert!(matches!(error, CliError::Unavailable(_)));
        assert!(error.to_string().contains(BUS_NAME));
    }
}
