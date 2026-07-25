//! New Releases plugin controls beyond the generic enable switch.

use std::cell::RefCell;
use std::rc::Rc;

use libadwaita as adw;
use libadwaita::prelude::*;
use rusqlite::Connection;

use super::strings;

pub(in crate::ui) fn scope_row(conn: &Rc<RefCell<Connection>>, enabled: bool) -> adw::ComboRow {
    let selected = reprise_core::artist_news::configured_fetch_scope(&conn.borrow()).map_or(
        0,
        |scope| {
            u32::from(matches!(
                scope,
                reprise_core::artist_news::FetchScope::AllArtists
            ))
        },
    );
    let model = gtk4::StringList::new(&[
        &strings::text(strings::TOP_ARTISTS_ONLY),
        &strings::text(strings::ALL_ARTISTS),
    ]);
    let row = adw::ComboRow::builder()
        .title(strings::text(strings::NEW_RELEASES_ARTISTS))
        .model(&model)
        .selected(selected)
        .sensitive(enabled)
        .build();
    let conn = conn.clone();
    row.connect_selected_notify(move |row| {
        if let Err(error) =
            reprise_core::artist_news::set_fetch_all_artists(&conn.borrow(), row.selected() == 1)
        {
            tracing::warn!(%error, "could not save New Releases artist scope");
        }
    });
    row
}

pub(in crate::ui) fn singles_row(conn: &Rc<RefCell<Connection>>, enabled: bool) -> adw::SwitchRow {
    let active = reprise_core::artist_news::include_singles(&conn.borrow()).unwrap_or(false);
    let row = adw::SwitchRow::builder()
        .title(strings::text(strings::NEW_RELEASES_INCLUDE_SINGLES))
        .subtitle(strings::text(strings::NEW_RELEASES_INCLUDE_SINGLES_DESCRIPTION))
        .active(active)
        .sensitive(enabled)
        .build();
    let conn = conn.clone();
    row.connect_active_notify(move |row| {
        if let Err(error) =
            reprise_core::artist_news::set_include_singles(&conn.borrow(), row.is_active())
        {
            tracing::warn!(%error, "could not save New Releases singles setting");
        }
    });
    row
}
