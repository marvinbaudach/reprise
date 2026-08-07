//! New Releases plugin controls beyond the generic enable switch.

use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::db::Db;

use crate::ui::artist_news_worker::ArtistNewsRuntime;

use super::strings;

#[derive(Clone)]
pub(in crate::ui) struct NewReleasePreferenceRows {
    rows: Rc<Vec<gtk4::Widget>>,
}

impl NewReleasePreferenceRows {
    pub(in crate::ui) fn add_to(&self, expander: &adw::ExpanderRow) {
        for row in self.rows.iter() {
            expander.add_row(row);
        }
    }

    pub(in crate::ui) fn set_sensitive(&self, enabled: bool) {
        for row in self.rows.iter() {
            row.set_sensitive(enabled);
        }
    }
}

pub(in crate::ui) fn build(
    conn: &Rc<Db>,
    runtime: &Rc<ArtistNewsRuntime>,
    enabled: bool,
) -> NewReleasePreferenceRows {
    let rows = NewReleasePreferenceRows {
        rows: Rc::new(vec![scope_row(conn, enabled).upcast()]),
    };
    let alive = rows.rows[0].downgrade();
    let target = rows.clone();
    runtime.subscribe_enabled(
        move || alive.upgrade().is_some(),
        move |enabled| target.set_sensitive(enabled),
    );
    rows
}

pub(in crate::ui) fn scope_row(conn: &Rc<Db>, enabled: bool) -> adw::ComboRow {
    let selected = reprise_core::artist_news::configured_fetch_scope(conn).map_or(0, |scope| {
        u32::from(matches!(
            scope,
            reprise_core::artist_news::FetchScope::AllArtists
        ))
    });
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
            reprise_core::artist_news::set_fetch_all_artists(&conn, row.selected() == 1)
        {
            tracing::warn!(%error, "could not save New Releases artist scope");
        }
    });
    row
}
