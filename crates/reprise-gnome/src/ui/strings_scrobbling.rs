//! Scrobbling-provider copy extracted from the central string catalog.

use super::{formatted, plural};

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

pub const PLUGIN_LISTENBRAINZ_DESCRIPTION: &str =
    N_!("Scrobble completed listens to ListenBrainz (network; off by default)");
pub const PLUGIN_LASTFM_DESCRIPTION: &str =
    N_!("Scrobble completed listens to Last.fm (network; off by default)");
pub const CONFIGURE: &str = N_!("Configure…");
pub const LISTENBRAINZ: &str = N_!("ListenBrainz");
pub const LISTENBRAINZ_ACCOUNT: &str = N_!("ListenBrainz Account");
pub const LISTENBRAINZ_NOT_CONNECTED: &str = N_!("Not connected");
pub const LISTENBRAINZ_CONNECTING: &str = N_!("Connecting…");
pub const LISTENBRAINZ_TOKEN_REJECTED: &str = N_!("Token rejected");
pub const LISTENBRAINZ_CONNECTION_ERROR: &str = N_!("Connection error");
pub const LISTENBRAINZ_OFFLINE: &str = N_!("Offline");
pub const LISTENBRAINZ_DIALOG_BODY: &str = N_!(
    "Enter a user token from your ListenBrainz profile. The token is stored in the system keyring."
);
pub const LISTENBRAINZ_TOKEN: &str = N_!("User token");
pub const LISTENBRAINZ_CONNECT: &str = N_!("Connect");
pub const LISTENBRAINZ_DISCONNECT: &str = N_!("Disconnect");
pub const LISTENBRAINZ_KEYRING_ERROR: &str =
    N_!("Could not access the system keyring. The token was not stored.");
pub const LISTENBRAINZ_VALIDATION_ERROR: &str =
    N_!("Could not validate the ListenBrainz token. Try again later.");
pub const LISTENBRAINZ_DISCONNECT_ERROR: &str =
    N_!("Could not remove the ListenBrainz token from the system keyring.");
pub const LASTFM: &str = N_!("Last.fm");
pub const LASTFM_ACCOUNT: &str = N_!("Last.fm Account");
pub const LASTFM_DIALOG_BODY: &str = N_!(
    "Enter credentials for a Last.fm desktop API application. They and the resulting session are stored only in the system keyring."
);
pub const LASTFM_API_KEY: &str = N_!("API key");
pub const LASTFM_SHARED_SECRET: &str = N_!("Shared secret");
pub const LASTFM_AUTHORIZE_HEADING: &str = N_!("Authorize Reprise in Your Browser");
pub const LASTFM_AUTHORIZE_BODY: &str =
    N_!("After approving access on Last.fm, return here and continue.");
pub const LASTFM_CONTINUE: &str = N_!("Continue");
pub const LASTFM_CREDENTIALS_REJECTED: &str = N_!("Credentials or authorization rejected");
pub const LASTFM_KEYRING_ERROR: &str =
    N_!("Could not access the system keyring. Last.fm credentials were not stored.");
pub const LASTFM_CONNECTION_ERROR: &str = N_!("Could not connect to Last.fm. Try again later.");
pub const LASTFM_DISCONNECT_ERROR: &str =
    N_!("Could not remove Last.fm credentials from the system keyring.");
pub const OPEN_BROWSER: &str = N_!("Open Browser");

pub fn listenbrainz_connected(user_name: &str) -> String {
    formatted(N_!("Connected as {user_name}"), &[("user_name", user_name)])
}

pub fn listenbrainz_pending(prefix: &str, pending: usize) -> String {
    let pending_text = pending.to_string();
    plural(
        "{prefix} · {pending} listen pending",
        "{prefix} · {pending} listens pending",
        pending,
        &[("prefix", prefix), ("pending", &pending_text)],
    )
}

pub fn lastfm_connected(user_name: &str) -> String {
    formatted(N_!("Connected as {user_name}"), &[("user_name", user_name)])
}

pub fn lastfm_pending(prefix: &str, pending: usize) -> String {
    let pending_text = pending.to_string();
    plural(
        "{prefix} · {pending} scrobble pending",
        "{prefix} · {pending} scrobbles pending",
        pending,
        &[("prefix", prefix), ("pending", &pending_text)],
    )
}
