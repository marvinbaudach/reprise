//! Small window-level verification hooks kept out of edge-tight `window.rs`.

use std::cell::RefCell;
use std::rc::Rc;

use libadwaita as adw;
use reprise_core::library::settings;
use rusqlite::Connection;

const SMOKE_BAR_POSITION_ENV_VAR: &str = "REPRISE_SMOKE_BAR_POSITION";
const SMOKE_LISTENBRAINZ_ENV_VAR: &str = "REPRISE_SMOKE_LISTENBRAINZ";
const SMOKE_LISTENBRAINZ_API_ROOT_ENV_VAR: &str = "REPRISE_SMOKE_LISTENBRAINZ_API_ROOT";

fn is_loopback_smoke_root(value: &str) -> bool {
    ["http://127.0.0.1:", "http://[::1]:"]
        .into_iter()
        .find_map(|prefix| value.strip_prefix(prefix))
        .and_then(|remainder| remainder.split('/').next())
        .is_some_and(|port| port.parse::<u16>().is_ok())
}

pub(super) fn arm_bar_position(
    conn: &Rc<RefCell<Connection>>,
    toolbar_view: &adw::ToolbarView,
    bottom_box: &gtk4::Box,
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
    super::window::apply_bar_position(toolbar_view, bottom_box, position);
    tracing::info!(position = %value, "smoke: applied player bar position");
}

/// Queues one synthetic listen and starts the worker only when a debug build
/// also has a validated loopback-only API override. The fixed fake token and
/// metadata never represent user data and the hook cannot redirect a release
/// build or a real keyring token.
pub(super) fn arm_listenbrainz(
    conn: &Rc<RefCell<Connection>>,
    runtime: &Rc<super::scrobble_runtime::ScrobbleRuntime>,
) {
    if std::env::var(SMOKE_LISTENBRAINZ_ENV_VAR).as_deref() != Ok("exercise") {
        return;
    }
    let Some(api_root) = std::env::var(SMOKE_LISTENBRAINZ_API_ROOT_ENV_VAR)
        .ok()
        .filter(|root| cfg!(debug_assertions) && is_loopback_smoke_root(root))
    else {
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

#[cfg(test)]
mod tests {
    use super::is_loopback_smoke_root;

    #[test]
    fn smoke_api_override_accepts_only_explicit_loopback_http_ports() {
        assert!(is_loopback_smoke_root("http://127.0.0.1:8123"));
        assert!(is_loopback_smoke_root("http://[::1]:8123/api"));
        assert!(!is_loopback_smoke_root("https://api.listenbrainz.org"));
        assert!(!is_loopback_smoke_root("http://127.0.0.1"));
        assert!(!is_loopback_smoke_root("http://example.test:8123"));
    }
}
