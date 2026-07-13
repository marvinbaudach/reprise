//! Small window-level verification hooks kept out of edge-tight `window.rs`.

use std::cell::RefCell;
use std::rc::Rc;

use libadwaita as adw;
use reprise_core::library::settings;
use rusqlite::Connection;

const SMOKE_BAR_POSITION_ENV_VAR: &str = "REPRISE_SMOKE_BAR_POSITION";
const SMOKE_LISTENBRAINZ_ENV_VAR: &str = "REPRISE_SMOKE_LISTENBRAINZ";

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
    runtime: &Rc<super::listenbrainz_runtime::ListenBrainzRuntime>,
) {
    if std::env::var(SMOKE_LISTENBRAINZ_ENV_VAR).as_deref() != Ok("exercise") {
        return;
    }
    if !runtime.smoke_api_is_local() {
        tracing::warn!("ListenBrainz smoke requested without a loopback API root");
        return;
    }
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
            runtime.configure("reprise-smoke-token".to_string());
            tracing::info!(queue_id, "smoke: queued synthetic ListenBrainz listen");
        }
        Err(error) => tracing::warn!(%error, "ListenBrainz smoke could not queue listen"),
    }
}
