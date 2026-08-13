//! Result rows for the radio Add Station dialog.

use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::db::Db;
use reprise_core::radio;
use reprise_core::radio::search::StationCandidate;

use super::add_dialog_network::{now_unix, station_from_candidate};
use super::images_allowed;
use crate::ui::search_highlight::{self, HighlightPalette};
use crate::ui::source_add_action;
use crate::ui::strings;

/// `RAD-6`: the only provider text searched by radio-browser is the station
/// name. `None` represents a shortcut-chip search, which has tag/country
/// criteria but no free-text needle.
pub(super) fn title_markup(
    station_name: &str,
    query: Option<&str>,
    palette: Option<&HighlightPalette>,
) -> Option<String> {
    query.and_then(|needle| search_highlight::highlight_markup(station_name, needle, palette))
}

pub(super) fn candidate_row(
    candidate: StationCandidate,
    query: Option<&str>,
    conn: &Rc<Db>,
    on_added: &Rc<dyn Fn()>,
) -> gtk4::ListBoxRow {
    let row = gtk4::ListBoxRow::new();
    let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    let tile = crate::ui::podcasts::source_image::SourceImage::new(
        candidate.favicon_url.as_deref(),
        "network-wireless-symbolic",
        40,
        images_allowed(conn),
        reprise_core::remote_image::CacheScope::Transient,
    );
    let copy = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    copy.set_hexpand(true);
    let title = gtk4::Label::new(Some(&candidate.name));
    title.set_xalign(0.0);
    // SRC-8: both lines ellipsize. A label that keeps its full text width
    // raises the dialog's minimum width, including rows outside the viewport.
    title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    if let Some(query) = query {
        let palette = search_highlight::accent_palette(&title);
        if let Some(markup) = title_markup(&candidate.name, Some(query), Some(&palette)) {
            title.set_markup(&markup);
        }
    }
    // RAD-6: details are generated metadata, not searched provider text. Keep
    // them on GTK's escaped plain-text path even when they contain the query.
    let details_text = radio::search::format_candidate_details(&candidate);
    let details = gtk4::Label::new(Some(&details_text));
    details.set_xalign(0.0);
    details.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    details.add_css_class("reprise-text-secondary");
    details.add_css_class("caption");
    copy.append(&title);
    copy.append(&details);

    // SRC-7: the same compact action the podcast and channel dialogs use.
    let station_name = candidate.name.clone();
    let add = source_add_action::add_button(source_add_action::AddActionKind::Add, &station_name);
    let conn = conn.clone();
    let on_added = on_added.clone();
    add.connect_clicked(move |button| {
        let station = station_from_candidate(candidate.clone());
        let result = radio::station::add_or_restore(&conn, &station, now_unix());
        match result {
            Ok(_) => {
                on_added();
                // SRC-7: acknowledge in place instead of removing the row,
                // so the add stays visible.
                source_add_action::mark_added(
                    button,
                    source_add_action::AddActionKind::Add,
                    &station_name,
                );
            }
            Err(error) => {
                tracing::warn!(%error, "could not add radio search result");
                button.set_tooltip_text(Some(&strings::text(strings::RADIO_ADD_FAILED)));
            }
        }
    });
    content.append(tile.widget());
    content.append(&copy);
    content.append(&add);
    row.set_child(Some(&content));
    row
}
