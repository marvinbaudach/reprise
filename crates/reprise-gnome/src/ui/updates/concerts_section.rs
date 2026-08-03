//! Concert delta presentation for the grouped Updates popover.

use std::cell::RefCell;
use std::rc::Rc;

use chrono::{Datelike, NaiveDate};
use gtk4::prelude::*;
use reprise_core::concerts::ConcertRow;

use crate::ui::strings;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct ConcertDeltaPresentation {
    pub artist: String,
    pub meta: String,
    pub distance: Option<String>,
    pub ticket_label: Option<String>,
    pub target: Option<String>,
}

pub(super) fn concerts_section_visible(enabled: bool, has_credentials: bool, total: usize) -> bool {
    enabled && has_credentials && total > 0
}

pub(super) fn delta_presentations(
    rows: &[ConcertRow],
    today: NaiveDate,
) -> Vec<ConcertDeltaPresentation> {
    rows.iter()
        .map(|row| {
            let date = NaiveDate::parse_from_str(&row.date_key, "%Y-%m-%d").map_or_else(
                |_| row.date_key.clone(),
                |date| {
                    if date.year() == today.year() {
                        date.format("%a, %-d %b").to_string()
                    } else {
                        date.format("%a, %-d %b %Y").to_string()
                    }
                },
            );
            let location = if row.city.trim().is_empty() {
                row.venue.clone()
            } else if row.venue.trim().is_empty() {
                row.city.clone()
            } else {
                format!("{} · {}", row.city, row.venue)
            };
            // Provider JSON decides these URLs, so only web links become a
            // clickable target; anything else leaves the row inert (CONC-7).
            let target = [row.ticket_url.as_deref(), row.event_url.as_deref()]
                .into_iter()
                .flatten()
                .find(|url| reprise_core::external_link::is_launchable_url(url))
                .map(str::to_owned);
            ConcertDeltaPresentation {
                artist: row.artist_name.clone(),
                meta: format!("{date} · {location}"),
                distance: row
                    .distance_km
                    .map(|distance| format!("{:.0} km", distance.max(0.0).round())),
                ticket_label: target.as_ref().map(|_| {
                    row.ticket_source
                        .as_deref()
                        .filter(|source| !source.trim().is_empty())
                        .map_or_else(|| strings::text(strings::CONCERTS_TICKETS), strings::text)
                }),
                target,
            }
        })
        .collect()
}

pub(super) type OnOpenUrl = Rc<dyn Fn(String)>;

pub(super) struct ConcertsSection {
    root: gtk4::Box,
    list: gtk4::Box,
    count_tag: gtk4::Label,
    on_open_url: RefCell<OnOpenUrl>,
}

impl ConcertsSection {
    pub(super) fn new() -> Self {
        let title = gtk4::Label::new(Some(&strings::text(strings::UPDATES_CONCERTS_HEADER)));
        title.add_css_class("new-release-header");
        title.set_xalign(0.0);
        title.set_hexpand(true);
        let count_tag = gtk4::Label::new(None);
        count_tag.add_css_class("new-release-tag");
        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        header.append(&title);
        header.append(&count_tag);

        let list = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        root.append(&header);
        root.append(&list);
        Self {
            root,
            list,
            count_tag,
            on_open_url: RefCell::new(Rc::new(|_| {})),
        }
    }

    pub(super) fn root(&self) -> &gtk4::Box {
        &self.root
    }

    pub(super) fn set_on_open_url(&self, on_open_url: OnOpenUrl) {
        *self.on_open_url.borrow_mut() = on_open_url;
    }

    pub(super) fn render(
        &self,
        enabled: bool,
        has_credentials: bool,
        total: usize,
        rows: &[ConcertRow],
        today: NaiveDate,
    ) {
        let visible = concerts_section_visible(enabled, has_credentials, total);
        self.root.set_visible(visible);
        if !visible {
            return;
        }
        self.count_tag.set_label(&strings::updates_new_count(total));
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        for row in delta_presentations(rows, today) {
            self.list
                .append(&build_delta_row(row, &self.on_open_url.borrow()));
        }
    }
}

fn build_delta_row(row: ConcertDeltaPresentation, on_open_url: &OnOpenUrl) -> gtk4::Button {
    let artist = gtk4::Label::new(Some(&row.artist));
    artist.set_xalign(0.0);
    artist.set_hexpand(true);
    let ticket = gtk4::Label::new(row.ticket_label.as_deref());
    ticket.add_css_class("dim-label");
    ticket.set_visible(row.ticket_label.is_some());
    let title = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    title.append(&artist);
    title.append(&ticket);

    let meta = gtk4::Label::new(Some(&row.meta));
    meta.set_xalign(0.0);
    meta.add_css_class("dim-label");
    let distance = gtk4::Label::new(row.distance.as_deref());
    distance.set_xalign(0.0);
    distance.add_css_class("dim-label");
    distance.set_visible(row.distance.is_some());
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    content.append(&title);
    content.append(&meta);
    content.append(&distance);
    let button = gtk4::Button::builder()
        .child(&content)
        .css_classes(["flat", "new-release-history-row"])
        .build();
    button.set_sensitive(row.target.is_some());
    if let Some(target) = row.target {
        let on_open_url = on_open_url.clone();
        button.connect_clicked(move |_| on_open_url(target.clone()));
    }
    button
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: i64, date: &str) -> ConcertRow {
        ConcertRow {
            id,
            date_key: date.into(),
            starts_at: format!("{date}T19:00:00"),
            artist_name: format!("Artist {id}"),
            venue: "Palladium".into(),
            city: "Cologne".into(),
            region: None,
            country: Some("DE".into()),
            latitude: None,
            longitude: None,
            distance_km: Some(38.2),
            ticket_url: Some(format!("https://tickets.example/{id}")),
            ticket_source: Some("Eventim".into()),
            event_url: Some(format!("https://events.example/{id}")),
            provider: "bandsintown".into(),
            is_similar: false,
            similar_to: None,
        }
    }

    #[test]
    fn conc_9_section_is_absent_without_provider_credentials() {
        assert!(concerts_section_visible(true, true, 1));
        assert!(!concerts_section_visible(true, false, 1));
        assert!(!concerts_section_visible(false, true, 1));
        assert!(!concerts_section_visible(true, true, 0));
    }

    #[test]
    fn non_web_provider_urls_leave_the_delta_row_without_a_ticket_target() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 25).unwrap();
        let mut hostile = row(1, "2026-08-01");
        hostile.ticket_url = Some("javascript:alert(1)".into());
        hostile.event_url = Some("file:///etc/passwd".into());

        let presentations = delta_presentations(&[hostile], today);

        assert_eq!(presentations[0].target, None);
        assert_eq!(presentations[0].ticket_label, None);
    }

    #[test]
    fn conc_7_delta_rows_preserve_the_already_capped_snapshot() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 25).unwrap();
        let rows = (1..=4)
            .map(|id| row(id, &format!("2026-08-0{id}")))
            .collect::<Vec<_>>();

        let presentations = delta_presentations(&rows, today);

        assert_eq!(presentations.len(), 4);
        assert_eq!(presentations[0].artist, "Artist 1");
        assert!(presentations[0].meta.contains("Cologne"));
        assert!(presentations[0].meta.contains("Palladium"));
        assert_eq!(presentations[0].distance.as_deref(), Some("38 km"));
        assert_eq!(presentations[0].ticket_label.as_deref(), Some("Eventim"));
        assert_eq!(
            presentations[0].target.as_deref(),
            Some("https://tickets.example/1")
        );
    }
}
