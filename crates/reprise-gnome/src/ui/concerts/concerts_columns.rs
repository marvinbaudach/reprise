#![allow(dead_code)]

use std::rc::Rc;

use chrono::Local;
use gtk4::prelude::*;
use reprise_core::concerts::ConcertRow;

use super::concerts_model::ConcertObject;
use super::concerts_presentation::{format_distance_km, format_event_date, ticket_button_label};
use crate::ui::strings;

pub(super) type OnOpenTarget = Rc<dyn Fn(String)>;

pub(super) fn ticket_target(row: &ConcertRow) -> Option<&str> {
    row.ticket_url.as_deref().or(row.event_url.as_deref())
}

fn city_tooltip(row: &ConcertRow) -> Option<String> {
    let details = [row.region.as_deref(), row.country.as_deref()]
        .into_iter()
        .flatten()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>();
    (!details.is_empty()).then(|| details.join(" · "))
}

fn similar_caption(row: &ConcertRow) -> Option<String> {
    row.is_similar
        .then_some(row.similar_to.as_deref())
        .flatten()
        .filter(|seed| !seed.trim().is_empty())
        .map(strings::concert_similar_caption)
}

fn artist_column(view: &gtk4::ColumnView) {
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let cell = gtk4::Box::new(gtk4::Orientation::Vertical, 1);
        let artist = gtk4::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .build();
        let caption = gtk4::Label::builder()
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .build();
        caption.add_css_class("dim-label");
        caption.add_css_class("caption");
        cell.append(&artist);
        cell.append(&caption);
        item.set_child(Some(&cell));
    });
    factory.connect_bind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(cell) = item.child().and_downcast::<gtk4::Box>() else {
            return;
        };
        let Some(artist) = cell.first_child().and_downcast::<gtk4::Label>() else {
            return;
        };
        let Some(caption) = artist.next_sibling().and_downcast::<gtk4::Label>() else {
            return;
        };
        let Some(object) = item.item().and_downcast::<ConcertObject>() else {
            return;
        };
        let row = object.row();
        artist.set_text(&row.artist_name);
        let text = similar_caption(&row);
        caption.set_text(text.as_deref().unwrap_or_default());
        caption.set_visible(text.is_some());
    });
    factory.connect_unbind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(cell) = item.child().and_downcast::<gtk4::Box>() else {
            return;
        };
        let Some(artist) = cell.first_child().and_downcast::<gtk4::Label>() else {
            return;
        };
        let Some(caption) = artist.next_sibling().and_downcast::<gtk4::Label>() else {
            return;
        };
        artist.set_text("");
        caption.set_text("");
        caption.set_visible(false);
    });
    let column = gtk4::ColumnViewColumn::builder()
        .title(strings::text(strings::CONCERTS_ARTIST))
        .factory(&factory)
        .resizable(true)
        .expand(true)
        .build();
    view.append_column(&column);
}

fn text_column(
    view: &gtk4::ColumnView,
    title: &str,
    id: Option<&str>,
    numeric: bool,
    render: impl Fn(&ConcertRow) -> String + 'static,
    tooltip: impl Fn(&ConcertRow) -> Option<String> + 'static,
) -> gtk4::ColumnViewColumn {
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let label = gtk4::Label::new(None);
        label.set_xalign(if numeric { 1.0 } else { 0.0 });
        label.set_hexpand(true);
        label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        if numeric {
            label.add_css_class("numeric");
        }
        item.set_child(Some(&label));
    });
    factory.connect_bind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(label) = item.child().and_downcast::<gtk4::Label>() else {
            return;
        };
        let Some(object) = item.item().and_downcast::<ConcertObject>() else {
            return;
        };
        let row = object.row();
        label.set_text(&render(&row));
        label.set_tooltip_text(tooltip(&row).as_deref());
    });
    factory.connect_unbind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(label) = item.child().and_downcast::<gtk4::Label>() else {
            return;
        };
        label.set_text("");
        label.set_tooltip_text(None);
    });

    let column = gtk4::ColumnViewColumn::builder()
        .title(title)
        .factory(&factory)
        .resizable(true)
        .expand(true)
        .build();
    if let Some(id) = id {
        column.set_id(Some(id));
        column.set_sorter(Some(&gtk4::CustomSorter::new(|_, _| gtk4::Ordering::Equal)));
    }
    view.append_column(&column);
    column
}

fn ticket_column(view: &gtk4::ColumnView, on_open: &OnOpenTarget) {
    let factory = gtk4::SignalListItemFactory::new();
    let on_open = on_open.clone();
    factory.connect_setup(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let cell = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        let button = gtk4::Button::new();
        button.add_css_class("flat");
        button.add_css_class("link");
        let item_weak = item.downgrade();
        let on_open = on_open.clone();
        button.connect_clicked(move |_| {
            let Some(item) = item_weak.upgrade() else {
                return;
            };
            let Some(object) = item.item().and_downcast::<ConcertObject>() else {
                return;
            };
            if let Some(target) = ticket_target(&object.row()) {
                on_open(target.to_owned());
            }
        });
        cell.append(&button);
        item.set_child(Some(&cell));
    });
    factory.connect_bind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(cell) = item.child().and_downcast::<gtk4::Box>() else {
            return;
        };
        let Some(button) = cell.first_child().and_downcast::<gtk4::Button>() else {
            return;
        };
        let Some(object) = item.item().and_downcast::<ConcertObject>() else {
            return;
        };
        let row = object.row();
        let label = ticket_button_label(&row);
        button.set_label(label.as_deref().unwrap_or_default());
        button.set_visible(label.is_some());
        cell.set_tooltip_text(Some(&ticket_target(&row).map_or_else(
            || strings::text(strings::CONCERTS_NO_LINK),
            ToOwned::to_owned,
        )));
        if let Some(label) = label {
            button.update_property(&[gtk4::accessible::Property::Label(&label)]);
        }
    });
    factory.connect_unbind(move |_, object| {
        let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(cell) = item.child().and_downcast::<gtk4::Box>() else {
            return;
        };
        let Some(button) = cell.first_child().and_downcast::<gtk4::Button>() else {
            return;
        };
        button.set_label("");
        cell.set_tooltip_text(None);
        button.set_visible(false);
    });

    let column = gtk4::ColumnViewColumn::builder()
        .title(strings::text(strings::CONCERTS_TICKETS))
        .factory(&factory)
        .resizable(false)
        .build();
    view.append_column(&column);
}

pub(super) struct SortColumns {
    pub date: gtk4::ColumnViewColumn,
    pub distance: gtk4::ColumnViewColumn,
}

pub(super) fn append_columns(view: &gtk4::ColumnView, on_open: &OnOpenTarget) -> SortColumns {
    let date = text_column(
        view,
        &strings::text(strings::CONCERTS_DATE),
        Some("date"),
        false,
        |row| format_event_date(&row.date_key, Local::now().date_naive()),
        |_| None,
    );
    artist_column(view);
    text_column(
        view,
        &strings::text(strings::CONCERTS_CITY),
        None,
        false,
        |row| row.city.clone(),
        city_tooltip,
    );
    text_column(
        view,
        &strings::text(strings::CONCERTS_VENUE),
        None,
        false,
        |row| row.venue.clone(),
        |_| None,
    );
    let distance = text_column(
        view,
        &strings::text(strings::CONCERTS_DISTANCE),
        Some("distance"),
        true,
        |row| format_distance_km(row.distance_km),
        |_| None,
    );
    ticket_column(view, on_open);
    SortColumns { date, distance }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(ticket_url: Option<&str>, event_url: Option<&str>) -> ConcertRow {
        ConcertRow {
            id: 1,
            date_key: "2026-10-17".into(),
            starts_at: "2026-10-17T19:00:00".into(),
            artist_name: "Lorna Shore".into(),
            venue: "Zenith".into(),
            city: "Munich".into(),
            region: Some("BY".into()),
            country: Some("DE".into()),
            latitude: None,
            longitude: None,
            distance_km: None,
            ticket_url: ticket_url.map(str::to_owned),
            ticket_source: Some("Ticketmaster".into()),
            event_url: event_url.map(str::to_owned),
            provider: "fixture".into(),
            is_similar: false,
            similar_to: None,
        }
    }

    #[test]
    fn conc_3_row_activation_opens_ticket_target_then_event_fallback() {
        let offer = row(
            Some("https://tickets.example/offer"),
            Some("https://events.example/event"),
        );
        assert_eq!(ticket_target(&offer), Some("https://tickets.example/offer"));
        let event = row(None, Some("https://events.example/event"));
        assert_eq!(ticket_target(&event), Some("https://events.example/event"));
        assert_eq!(ticket_target(&row(None, None)), None);
    }

    #[test]
    fn city_tooltip_joins_only_available_location_context() {
        assert_eq!(city_tooltip(&row(None, None)).as_deref(), Some("BY · DE"));
    }

    #[test]
    fn conc_6_similar_rows_carry_seed_caption() {
        let mut event = row(None, None);
        event.is_similar = true;
        event.similar_to = Some("Bring Me the Horizon".into());
        assert_eq!(
            similar_caption(&event).as_deref(),
            Some("similar to Bring Me the Horizon")
        );
        event.is_similar = false;
        assert_eq!(similar_caption(&event), None);
    }
}
