//! Internet Radio source surface.
#![allow(dead_code)]

mod add_dialog;
mod add_dialog_location;
mod add_dialog_network;
mod add_dialog_rows;
mod css;
mod edit_dialog;
mod radio_add_input;
mod radio_artwork_refresh;
mod radio_chips;
mod radio_column_layout;
mod radio_columns;
#[cfg(test)]
mod radio_columns_artwork_tests;
mod radio_context_menu;
mod radio_empty_state;
mod radio_filter_bar;
mod radio_filter_model;
mod radio_live_cells;
mod radio_location;
mod radio_model;
mod radio_presentation;
mod radio_reveal;
mod radio_view;
mod radio_view_search;
#[cfg(test)]
mod radio_view_test_hooks;
mod station_preview;

pub(in crate::ui) use radio_view::RadioView;
#[cfg(test)]
pub(in crate::ui) use radio_view_test_hooks::RadioTestHandle;

use std::rc::Rc;

use reprise_core::db::Db;

use crate::ui::playback::player_controller::PlayerController;

/// `NET-1a` / `SRC-11`: compute the Radio image gate fresh at every call so
/// callers never retain a stale consent snapshot.
pub(super) fn images_allowed(db: &Db) -> bool {
    reprise_core::online_sources::network_allowed(db, &reprise_core::modules::ARTWORK_MODULE)
        .unwrap_or(false)
}

pub(in crate::ui) fn install(
    conn: Rc<Db>,
    controller: Option<&Rc<PlayerController>>,
    location_broadcast: &Rc<crate::ui::location_broadcast::LocationBroadcast>,
) -> RadioView {
    let view = RadioView::new(conn, controller);
    if let Some(controller) = controller {
        radio_view::on_external_snapshot(&view.shared, controller.current_external_snapshot());
    }
    radio_location::subscribe(&view, location_broadcast);
    view
}

pub(in crate::ui) fn css() -> String {
    css::css()
}
