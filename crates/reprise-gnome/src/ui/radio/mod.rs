//! Internet Radio source surface.
#![allow(dead_code)]

mod add_dialog;
mod css;
mod edit_dialog;
mod radio_chips;
mod radio_columns;
mod radio_context_menu;
mod radio_empty_state;
mod radio_filter_bar;
mod radio_model;
mod radio_presentation;
mod radio_view;
mod station_preview;

pub(in crate::ui) use radio_view::RadioView;

use std::rc::Rc;

use reprise_core::db::Db;

use crate::ui::playback::player_controller::PlayerController;

pub(in crate::ui) fn install(conn: Rc<Db>, controller: Option<&Rc<PlayerController>>) -> RadioView {
    RadioView::new(conn, controller)
}

pub(in crate::ui) fn css() -> String {
    css::css()
}
