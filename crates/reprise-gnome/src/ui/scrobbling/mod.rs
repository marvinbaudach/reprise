pub(in crate::ui) mod lastfm_secret;
pub(in crate::ui) mod listenbrainz_secret;
pub(in crate::ui) mod scrobble_runtime;
pub(in crate::ui) mod scrobble_session;
pub(in crate::ui) mod smoke;

/// Native installs used this application ID for keyring attributes before the
/// Flathub rename. Remove this fallback a few releases after the transition.
const LEGACY_APP_ID: &str = "org.reprise.Reprise";

#[allow(unused_imports)]
use super::*;
