//! Concert delta presentation for the grouped Updates popover.

use std::cell::RefCell;
use std::rc::Rc;

use chrono::NaiveDate;
use gtk4::prelude::*;
use reprise_core::concerts::{ConcertRow, TicketAvailability};

use super::feed_row;
use super::release_cover::LazyReleaseCover;
use crate::ui::concerts::concerts_presentation::format_event_date;
use crate::ui::strings;

const COVER_EDGE: i32 = 44;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct ConcertDeltaPresentation {
    pub id: i64,
    pub artist: String,
    pub similar_caption: Option<String>,
    pub meta: String,
    pub tag: Option<feed_row::Tag>,
    pub tooltip: String,
    pub target: Option<String>,
}

pub(super) fn concerts_section_visible(enabled: bool) -> bool {
    enabled
}

fn source_name(row: &ConcertRow) -> &str {
    row.ticket_source
        .as_deref()
        .filter(|source| !source.trim().is_empty())
        .unwrap_or(row.provider.as_str())
}

pub(super) fn delta_presentations(
    rows: &[ConcertRow],
    today: NaiveDate,
) -> Vec<ConcertDeltaPresentation> {
    rows.iter()
        .map(|row| {
            let date = format_event_date(&row.date_key, today);
            let target = [row.ticket_url.as_deref(), row.event_url.as_deref()]
                .into_iter()
                .flatten()
                .find(|url| reprise_core::external_link::is_launchable_url(url))
                .map(str::to_owned);
            let tooltip = target.as_ref().map_or_else(
                || strings::text(strings::CONCERTS_NO_LINK),
                |_| strings::updates_opens_source(source_name(row)),
            );
            ConcertDeltaPresentation {
                id: row.id,
                artist: row.artist_name.clone(),
                similar_caption: row
                    .is_similar
                    .then_some(row.similar_to.as_deref())
                    .flatten()
                    .filter(|seed| !seed.trim().is_empty())
                    .map(strings::concert_similar_caption),
                meta: strings::updates_concert_meta(&date, &row.city, &row.venue),
                tag: (row.availability == TicketAvailability::OffSale).then(|| feed_row::Tag {
                    text: strings::text(strings::CONCERTS_OFF_SALE),
                    tone: feed_row::TagTone::Neutral,
                }),
                tooltip,
                target,
            }
        })
        .collect()
}

pub(super) type OnOpenUrl = Rc<dyn Fn(String)>;
pub(super) type OnDismissEvent = Rc<dyn Fn(i64)>;
pub(super) type OnOpenView = Rc<dyn Fn()>;

pub(super) struct ConcertsSection {
    root: gtk4::Box,
    header: gtk4::Button,
    list: gtk4::Box,
    count_tag: gtk4::Label,
    empty: gtk4::Label,
    on_open_url: RefCell<OnOpenUrl>,
    on_dismiss_event: RefCell<OnDismissEvent>,
    on_open_view: Rc<RefCell<OnOpenView>>,
}

impl ConcertsSection {
    pub(super) fn new() -> Self {
        let title = gtk4::Label::new(Some(&strings::text(strings::CONCERTS)));
        title.add_css_class("new-release-header");
        title.set_xalign(0.0);
        title.set_hexpand(true);
        let count_tag = gtk4::Label::new(None);
        count_tag.add_css_class("new-release-tag");
        let header_content = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        header_content.append(&title);
        header_content.append(&count_tag);
        let header = gtk4::Button::builder()
            .child(&header_content)
            .css_classes(["flat", "updates-section-header"])
            .build();

        let list = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        let empty = gtk4::Label::new(Some(&strings::text(strings::UPDATES_NO_NEW_CONCERTS)));
        empty.add_css_class("reprise-text-secondary");
        empty.set_xalign(0.0);
        empty.set_margin_top(8);
        empty.set_margin_bottom(8);
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        root.append(&header);
        root.append(&list);
        root.append(&empty);

        let section = Self {
            root,
            header,
            list,
            count_tag,
            empty,
            on_open_url: RefCell::new(Rc::new(|_| {})),
            on_dismiss_event: RefCell::new(Rc::new(|_| {})),
            on_open_view: Rc::new(RefCell::new(Rc::new(|| {}))),
        };
        section.wire_header();
        section
    }

    fn wire_header(&self) {
        let on_open_view = self.on_open_view.clone();
        self.header.connect_clicked(move |_| {
            let callback = on_open_view.borrow().clone();
            callback();
        });
    }

    pub(super) fn root(&self) -> &gtk4::Box {
        &self.root
    }

    #[cfg(test)]
    pub(super) fn count_tag(&self) -> &gtk4::Label {
        &self.count_tag
    }

    #[cfg(test)]
    pub(super) fn empty_label(&self) -> &gtk4::Label {
        &self.empty
    }

    #[cfg(test)]
    pub(super) fn header(&self) -> &gtk4::Button {
        &self.header
    }

    pub(super) fn set_on_open_url(&self, on_open_url: OnOpenUrl) {
        *self.on_open_url.borrow_mut() = on_open_url;
    }

    pub(super) fn set_on_dismiss_event(&self, on_dismiss_event: OnDismissEvent) {
        *self.on_dismiss_event.borrow_mut() = on_dismiss_event;
    }

    pub(super) fn set_on_open_view(&self, on_open_view: OnOpenView) {
        *self.on_open_view.borrow_mut() = on_open_view;
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn render(
        &self,
        enabled: bool,
        _has_credentials: bool,
        total: usize,
        unseen: bool,
        rows: &[ConcertRow],
        today: NaiveDate,
        cached_portraits: bool,
    ) {
        self.root.set_visible(concerts_section_visible(enabled));
        if !enabled {
            return;
        }
        self.count_tag.set_label(&strings::updates_new_count(total));
        self.count_tag.set_visible(unseen && total > 0);
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        let presentations = delta_presentations(rows, today);
        self.empty.set_visible(presentations.is_empty());
        for row in presentations {
            self.list.append(&build_delta_row(
                row,
                cached_portraits,
                &self.on_open_url.borrow(),
                &self.on_dismiss_event.borrow(),
            ));
        }
    }
}

fn build_delta_row(
    row: ConcertDeltaPresentation,
    cached_portraits: bool,
    on_open_url: &OnOpenUrl,
    on_dismiss_event: &OnDismissEvent,
) -> gtk4::Box {
    let cover = LazyReleaseCover::new_cached_artist(&row.artist, COVER_EDGE, cached_portraits);
    let target = row.target.clone();
    let on_open_url = on_open_url.clone();
    let on_activate: Rc<dyn Fn()> = Rc::new(move || {
        if let Some(target) = target.as_ref() {
            on_open_url(target.clone());
        }
    });
    let id = row.id;
    let on_dismiss_event = on_dismiss_event.clone();
    feed_row::build(
        cover.widget(),
        feed_row::Presentation {
            title: row.artist,
            title_suffix: row.similar_caption,
            meta: row.meta,
            tag: row.tag,
            tooltip: row.tooltip,
            activatable: row.target.is_some(),
        },
        &strings::text(strings::DISMISS),
        on_activate,
        Rc::new(move || on_dismiss_event(id)),
    )
    .root
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
            availability: TicketAvailability::Unknown,
        }
    }

    #[test]
    fn section_follows_the_module_even_without_rows_or_credentials() {
        assert!(concerts_section_visible(true));
        assert!(!concerts_section_visible(false));
    }

    #[test]
    fn non_web_provider_urls_leave_the_delta_row_without_a_ticket_target() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 25).unwrap();
        let mut hostile = row(1, "2026-08-01");
        hostile.ticket_url = Some("javascript:alert(1)".into());
        hostile.event_url = Some("file:///etc/passwd".into());

        let presentations = delta_presentations(&[hostile], today);

        assert_eq!(presentations[0].target, None);
        assert_eq!(
            presentations[0].tooltip,
            strings::text(strings::CONCERTS_NO_LINK)
        );
    }

    #[test]
    fn concert_fields_and_target_follow_the_shared_row_contract() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 25).unwrap();
        let mut event = row(1, "2026-08-01");
        event.is_similar = true;
        event.similar_to = Some("Seed".into());
        event.availability = TicketAvailability::OffSale;

        let presentation = delta_presentations(&[event], today).remove(0);

        assert_eq!(presentation.artist, "Artist 1");
        assert_eq!(
            presentation.similar_caption.as_deref(),
            Some("similar to Seed")
        );
        assert!(presentation.meta.contains("Cologne"));
        assert!(presentation.meta.contains("Palladium"));
        assert_eq!(
            presentation.tag.as_ref().map(|tag| tag.text.as_str()),
            Some("Off sale")
        );
        assert_eq!(presentation.tooltip, "Opens Eventim");
        assert_eq!(
            presentation.target.as_deref(),
            Some("https://tickets.example/1")
        );
    }
}
