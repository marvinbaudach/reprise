//! Small window-level verification hooks kept out of edge-tight `window.rs`.

use std::cell::RefCell;
use std::rc::Rc;

use libadwaita as adw;
use reprise_core::library::settings;
use rusqlite::Connection;

const SMOKE_BAR_POSITION_ENV_VAR: &str = "REPRISE_SMOKE_BAR_POSITION";

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
