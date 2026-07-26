//! Small window-level verification hooks kept out of edge-tight `window.rs`.

use std::cell::RefCell;
use std::rc::Rc;

use reprise_core::library::settings;
use rusqlite::Connection;

const SMOKE_BAR_POSITION_ENV_VAR: &str = "REPRISE_SMOKE_BAR_POSITION";
const SMOKE_LISTENBRAINZ_ENV_VAR: &str = "REPRISE_SMOKE_LISTENBRAINZ";
const SMOKE_LASTFM_ENV_VAR: &str = "REPRISE_SMOKE_LASTFM";

pub(in crate::ui) fn arm_bar_position(
    conn: &Rc<RefCell<Connection>>,
    library_player_bar: &super::library_player_bar::LibraryPlayerBarShell,
) {
    let Ok(value) = std::env::var(SMOKE_BAR_POSITION_ENV_VAR) else {
        return;
    };
    let position = if value == "top" {
        settings::PlayerBarPosition::Top
    } else {
        settings::PlayerBarPosition::Bottom
    };
    {
        let conn = conn.borrow();
        let _ = settings::set_player_bar_position(&conn, position);
    }
    library_player_bar.set_position(position);
    tracing::info!(position = %value, "smoke: applied player bar position");
}

/// Queues one synthetic listen and starts the worker only when a debug build
/// also has a validated loopback-only API override. The fixed fake token and
/// metadata never represent user data and the hook cannot redirect a release
/// build or a real keyring token.
pub(in crate::ui) fn arm_listenbrainz(
    conn: &Rc<RefCell<Connection>>,
    runtime: &Rc<super::scrobble_runtime::ScrobbleRuntime>,
) {
    if std::env::var(SMOKE_LISTENBRAINZ_ENV_VAR).as_deref() != Ok("exercise") {
        return;
    }
    let Some(api_root) = crate::ui::scrobbling::smoke::listenbrainz_api_root() else {
        tracing::warn!("ListenBrainz smoke requested without a loopback API root");
        return;
    };
    let listen = reprise_core::scrobbling::Listen {
        id: None,
        listened_at: 1_700_000_000,
        track: reprise_core::scrobbling::TrackMetadata {
            artist_name: "Reprise Smoke Artist".to_string(),
            track_name: "Reprise Smoke Track".to_string(),
            release_name: Some("Reprise Smoke Release".to_string()),
            duration_ms: 120_000,
        },
    };
    match reprise_core::scrobbling::enqueue(&conn.borrow(), &listen) {
        Ok(queue_id) => {
            runtime.configure(
                "reprise-smoke-token".to_string(),
                Box::new(reprise_core::scrobbling::ListenBrainzClient::with_api_root(
                    &api_root,
                )),
            );
            tracing::info!(queue_id, "smoke: queued synthetic ListenBrainz listen");
        }
        Err(error) => tracing::warn!(%error, "ListenBrainz smoke could not queue listen"),
    }
}

/// Exercises Last.fm signing and queue draining only against an explicit
/// debug-build loopback endpoint with synthetic credentials and metadata.
pub(in crate::ui) fn arm_lastfm(
    conn: &Rc<RefCell<Connection>>,
    runtime: &Rc<super::scrobble_runtime::ScrobbleRuntime>,
) {
    if std::env::var(SMOKE_LASTFM_ENV_VAR).as_deref() != Ok("exercise") {
        return;
    }
    let Some(api_root) = crate::ui::scrobbling::smoke::lastfm_api_root() else {
        tracing::warn!("Last.fm smoke requested without a loopback API root");
        return;
    };
    let listen = reprise_core::scrobbling::Listen {
        id: None,
        listened_at: 1_700_000_000,
        track: reprise_core::scrobbling::TrackMetadata {
            artist_name: "Reprise Last.fm Smoke Artist".to_string(),
            track_name: "Reprise Last.fm Smoke Track".to_string(),
            release_name: Some("Reprise Last.fm Smoke Release".to_string()),
            duration_ms: 120_000,
        },
    };
    let client = reprise_core::scrobbling::LastFmClient::with_roots(
        &api_root,
        &api_root,
        "reprise-smoke-api-key",
        "reprise-smoke-shared-secret",
    );
    match (
        reprise_core::scrobbling::enqueue_for(
            &conn.borrow(),
            reprise_core::scrobbling::ScrobbleProvider::LastFm,
            &listen,
        ),
        client,
    ) {
        (Ok(queue_id), Ok(client)) => {
            runtime.configure("reprise-smoke-session-key".to_string(), Box::new(client));
            tracing::info!(queue_id, "smoke: queued synthetic Last.fm scrobble");
        }
        (Err(error), _) => tracing::warn!(%error, "Last.fm smoke could not queue scrobble"),
        (_, Err(error)) => tracing::warn!(%error, "Last.fm smoke client is invalid"),
    }
}
