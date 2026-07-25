//! One-shot city-level location through the XDG Location portal.

use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

pub const ACCURACY_CITY: u32 = 2;
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const FLATPAK_INFO: &str = "/.flatpak-info";
const PORTAL_DESTINATION: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const LOCATION_INTERFACE: &str = "org.freedesktop.portal.Location";
const REQUEST_INTERFACE: &str = "org.freedesktop.portal.Request";
const SESSION_INTERFACE: &str = "org.freedesktop.portal.Session";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortalLocation {
    pub latitude: f64,
    pub longitude: f64,
    pub accuracy_m: Option<f64>,
}

pub fn location_from_vardict(
    values: &HashMap<String, OwnedValue>,
) -> Option<(f64, f64, Option<f64>)> {
    let latitude = f64::try_from(values.get("Latitude")?).ok()?;
    let longitude = f64::try_from(values.get("Longitude")?).ok()?;
    if !latitude.is_finite()
        || !longitude.is_finite()
        || !(-90.0..=90.0).contains(&latitude)
        || !(-180.0..=180.0).contains(&longitude)
    {
        return None;
    }
    let accuracy = values
        .get("Accuracy")
        .and_then(|value| f64::try_from(value).ok())
        .filter(|value| value.is_finite() && *value >= 0.0);
    Some((latitude, longitude, accuracy))
}

enum PortalMessage {
    Setup {
        connection: zbus::blocking::Connection,
        session_path: OwnedObjectPath,
    },
    Finished(Result<PortalLocation, String>),
}

/// Requests one city-level location update and closes the portal session.
pub fn current_location(timeout: Duration) -> Result<PortalLocation, String> {
    if timeout.is_zero() {
        return Err("Location portal timed out before CreateSession".to_string());
    }
    let (sender, receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("reprise-location-portal".to_string())
        .spawn(move || portal_worker(&sender))
        .map_err(|error| format!("could not start Location portal worker: {error}"))?;

    let started = Instant::now();
    let mut session = None;
    loop {
        let remaining = timeout.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            close_after_timeout(session);
            return Err(location_timeout_error());
        }
        match receiver.recv_timeout(remaining) {
            Ok(PortalMessage::Setup {
                connection,
                session_path,
            }) => session = Some((connection, session_path)),
            Ok(PortalMessage::Finished(result)) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                close_after_timeout(session);
                return Err(location_timeout_error());
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("Location portal worker ended without a result".to_string());
            }
        }
    }
}

fn portal_worker(sender: &mpsc::Sender<PortalMessage>) {
    let result = portal_roundtrip(sender);
    let _ = sender.send(PortalMessage::Finished(result));
}

fn portal_roundtrip(sender: &mpsc::Sender<PortalMessage>) -> Result<PortalLocation, String> {
    let connection = zbus::blocking::Connection::session().map_err(|error| {
        format!(
            "could not connect to session bus for Location portal ({}): {error}",
            environment_label()
        )
    })?;
    let proxy = zbus::blocking::Proxy::new(
        &connection,
        PORTAL_DESTINATION,
        PORTAL_PATH,
        LOCATION_INTERFACE,
    )
    .map_err(|error| format!("could not create Location portal proxy: {error}"))?;
    let token = request_token("session");
    let options = HashMap::from([
        ("session_handle_token", Value::from(token.as_str())),
        ("accuracy", Value::from(ACCURACY_CITY)),
    ]);
    let session_path: OwnedObjectPath = proxy
        .call("CreateSession", &options)
        .map_err(|error| format!("Location portal CreateSession failed: {error}"))?;
    let session_text = session_path.as_str().to_string();
    let mut signals = proxy
        .receive_signal_with_args("LocationUpdated", &[(0, session_text.as_str())])
        .map_err(|error| format!("could not subscribe to LocationUpdated: {error}"))?;
    let handle_token = request_token("request");
    let request_path = request_path(&connection, &handle_token)?;
    let request_proxy = zbus::blocking::Proxy::new(
        &connection,
        PORTAL_DESTINATION,
        request_path.as_str(),
        REQUEST_INTERFACE,
    )
    .map_err(|error| format!("could not create Location Start request proxy: {error}"))?;
    let mut responses = request_proxy
        .receive_signal("Response")
        .map_err(|error| format!("could not subscribe to Location Start response: {error}"))?;
    let start_options = HashMap::from([("handle_token", Value::from(handle_token.as_str()))]);
    let returned_request: OwnedObjectPath = proxy
        .call("Start", &(&session_path, "", &start_options))
        .map_err(|error| {
            close_session(&connection, &session_path);
            format!("Location portal Start failed: {error}")
        })?;
    if returned_request != request_path {
        close_session(&connection, &session_path);
        return Err("Location portal Start returned an unexpected request handle".to_string());
    }
    let _ = sender.send(PortalMessage::Setup {
        connection: connection.clone(),
        session_path: session_path.clone(),
    });
    let response = responses
        .next()
        .ok_or_else(|| "Location portal Start response stream ended".to_string())
        .and_then(|message| {
            let (code, _results): (u32, HashMap<String, OwnedValue>) = message
                .body()
                .deserialize()
                .map_err(|error| format!("Location portal Start response was invalid: {error}"))?;
            portal_response_result(code)
        });
    if let Err(error) = response {
        close_session(&connection, &session_path);
        return Err(error);
    }
    let result = signals
        .next()
        .ok_or_else(|| "LocationUpdated signal stream ended".to_string())
        .and_then(|message| {
            let (_path, values): (OwnedObjectPath, HashMap<String, OwnedValue>) = message
                .body()
                .deserialize()
                .map_err(|error| format!("LocationUpdated payload was invalid: {error}"))?;
            let (latitude, longitude, accuracy_m) = location_from_vardict(&values)
                .ok_or_else(|| "LocationUpdated payload had no valid coordinates".to_string())?;
            Ok(PortalLocation {
                latitude,
                longitude,
                accuracy_m,
            })
        });
    close_session(&connection, &session_path);
    result
}

fn request_path(
    connection: &zbus::blocking::Connection,
    token: &str,
) -> Result<OwnedObjectPath, String> {
    let sender = connection
        .unique_name()
        .ok_or_else(|| "session bus did not assign a unique name".to_string())?
        .as_str()
        .trim_start_matches(':')
        .replace('.', "_");
    OwnedObjectPath::try_from(format!("{PORTAL_PATH}/request/{sender}/{token}"))
        .map_err(|error| format!("could not build Location Start request path: {error}"))
}

fn portal_response_result(code: u32) -> Result<(), String> {
    match code {
        0 => Ok(()),
        1 => Err("Location portal Start was cancelled by the user".to_string()),
        other => Err(format!(
            "Location portal Start failed with response code {other}"
        )),
    }
}

fn close_after_timeout(session: Option<(zbus::blocking::Connection, OwnedObjectPath)>) {
    let Some((connection, session_path)) = session else {
        return;
    };
    close_session(&connection, &session_path);
    if let Err(error) = connection.close() {
        tracing::warn!(%error, "could not close timed-out Location portal connection");
    }
}

fn close_session(connection: &zbus::blocking::Connection, path: &OwnedObjectPath) {
    let result = zbus::blocking::Proxy::new(
        connection,
        PORTAL_DESTINATION,
        path.as_str(),
        SESSION_INTERFACE,
    )
    .and_then(|proxy| proxy.call::<_, _, ()>("Close", &()));
    if let Err(error) = result {
        tracing::warn!(%error, "could not close Location portal session");
    }
}

fn request_token(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{prefix}_{}_{}", std::process::id(), nanos)
}

fn environment_label() -> &'static str {
    if Path::new(FLATPAK_INFO).is_file() {
        "Flatpak sandbox"
    } else {
        "host session"
    }
}

fn location_timeout_error() -> String {
    format!(
        "Location portal timed out waiting for LocationUpdated ({})",
        environment_label()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn city_accuracy_and_timeout_policy_are_stable() {
        assert_eq!(ACCURACY_CITY, 2);
        assert_eq!(DEFAULT_TIMEOUT, Duration::from_secs(30));
    }

    #[test]
    fn location_vardict_requires_finite_coordinates_and_keeps_optional_accuracy() {
        let complete = HashMap::from([
            ("Latitude".to_string(), OwnedValue::from(47.3769_f64)),
            ("Longitude".to_string(), OwnedValue::from(8.5417_f64)),
            ("Accuracy".to_string(), OwnedValue::from(1_200_f64)),
        ]);
        assert_eq!(
            location_from_vardict(&complete),
            Some((47.3769, 8.5417, Some(1_200.0)))
        );
        let missing = HashMap::from([("Latitude".to_string(), OwnedValue::from(47.3769_f64))]);
        assert_eq!(location_from_vardict(&missing), None);
        let invalid = HashMap::from([
            ("Latitude".to_string(), OwnedValue::from(f64::NAN)),
            ("Longitude".to_string(), OwnedValue::from(8.5417_f64)),
        ]);
        assert_eq!(location_from_vardict(&invalid), None);
    }

    #[test]
    fn zero_timeout_fails_before_portal_access() {
        assert!(current_location(Duration::ZERO)
            .unwrap_err()
            .contains("timed out before CreateSession"));
    }

    #[test]
    fn portal_start_response_distinguishes_success_cancel_and_failure() {
        assert_eq!(portal_response_result(0), Ok(()));
        assert!(portal_response_result(1).unwrap_err().contains("cancelled"));
        assert!(portal_response_result(2)
            .unwrap_err()
            .contains("response code 2"));
    }
}
