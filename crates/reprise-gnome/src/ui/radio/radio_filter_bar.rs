use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::radio::StationRow;
use rusqlite::Connection;

use crate::ui::browse::browse_bar::CHIP_CSS_CLASS;
use crate::ui::strings;
use crate::ui::style::buttons;

const GENRE_KEY: &str = "radio.filter.genre";
const COUNTRY_KEY: &str = "radio.filter.country";
const FILTER_BAR_MIN_HEIGHT: i32 = 44;

type FilterCallback = Rc<dyn Fn(RadioFilter)>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct RadioFilter {
    pub genre: Option<String>,
    pub country: Option<String>,
}

impl RadioFilter {
    pub(super) fn is_active(&self) -> bool {
        self.genre.is_some() || self.country.is_some()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RadioFilterFacet {
    Genre,
    Country,
}

pub(super) fn remove_filter(filter: &RadioFilter, facet: RadioFilterFacet) -> RadioFilter {
    let mut result = filter.clone();
    match facet {
        RadioFilterFacet::Genre => result.genre = None,
        RadioFilterFacet::Country => result.country = None,
    }
    result
}

pub(super) fn filter_rows(rows: &[StationRow], filter: &RadioFilter) -> Vec<StationRow> {
    rows.iter()
        .filter(|row| {
            matches_value(row.genre.as_deref(), filter.genre.as_deref())
                && matches_value(row.country_code.as_deref(), filter.country.as_deref())
        })
        .cloned()
        .collect()
}

pub(super) fn genre_facets(rows: &[StationRow]) -> Vec<String> {
    facets(rows.iter().filter_map(|row| row.genre.as_deref()))
}

pub(super) fn country_facets(rows: &[StationRow]) -> Vec<String> {
    facets(rows.iter().filter_map(|row| row.country_code.as_deref()))
        .into_iter()
        .map(|value| value.to_ascii_uppercase())
        .collect()
}

fn facets<'a>(values: impl Iterator<Item = &'a str>) -> Vec<String> {
    values
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .fold(BTreeMap::<String, String>::new(), |mut values, value| {
            values
                .entry(value.to_lowercase())
                .or_insert_with(|| value.to_owned());
            values
        })
        .into_values()
        .collect()
}

fn matches_value(candidate: Option<&str>, expected: Option<&str>) -> bool {
    let Some(expected) = expected else {
        return true;
    };
    candidate.is_some_and(|candidate| candidate.trim().eq_ignore_ascii_case(expected.trim()))
}

pub(super) fn load_filter(conn: &Connection) -> Result<RadioFilter, rusqlite::Error> {
    Ok(RadioFilter {
        genre: setting(conn, GENRE_KEY)?,
        country: setting(conn, COUNTRY_KEY)?,
    })
}

pub(super) fn persist_filter(
    conn: &Connection,
    filter: &RadioFilter,
) -> Result<(), rusqlite::Error> {
    persist_value(conn, GENRE_KEY, filter.genre.as_deref())?;
    persist_value(conn, COUNTRY_KEY, filter.country.as_deref())
}

fn setting(conn: &Connection, key: &str) -> Result<Option<String>, rusqlite::Error> {
    Ok(reprise_core::library::settings::get_setting(conn, key)?
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty()))
}

fn persist_value(conn: &Connection, key: &str, value: Option<&str>) -> Result<(), rusqlite::Error> {
    reprise_core::library::settings::set_setting(conn, key, value.unwrap_or_default())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FilterChoice {
    facet: RadioFilterFacet,
    value: String,
}

pub(super) struct RadioFilterBar {
    root: gtk4::Box,
    add: gtk4::Button,
    add_filter: gtk4::MenuButton,
    chips: gtk4::Box,
    count: gtk4::Label,
    chooser: gtk4::ListBox,
    choices: RefCell<Vec<FilterChoice>>,
    conn: Rc<RefCell<Connection>>,
    filter: RefCell<RadioFilter>,
    visible_count: Cell<usize>,
    total_count: Cell<usize>,
    on_changed: RefCell<Option<FilterCallback>>,
}

impl RadioFilterBar {
    pub(super) fn new(conn: Rc<RefCell<Connection>>) -> Rc<Self> {
        let add = gtk4::Button::new();
        let add_content = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        add_content.append(&gtk4::Image::from_icon_name("list-add-symbolic"));
        add_content.append(&gtk4::Label::new(Some(&strings::text(strings::RADIO_ADD))));
        add.set_child(Some(&add_content));
        buttons::arm(&add, buttons::ADD_ACTION_CLASS);

        let add_filter = gtk4::MenuButton::builder()
            .label(format!("+ {}", strings::text(strings::RADIO_ADD_FILTER)))
            .build();
        add_filter.add_css_class(CHIP_CSS_CLASS);
        let chooser = gtk4::ListBox::new();
        chooser.set_selection_mode(gtk4::SelectionMode::None);
        let popover = gtk4::Popover::builder().child(&chooser).build();
        add_filter.set_popover(Some(&popover));

        let chips = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        let count = gtk4::Label::new(None);
        count.add_css_class("dim-label");
        count.add_css_class("caption");
        count.set_hexpand(true);
        count.set_halign(gtk4::Align::End);

        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        root.set_height_request(FILTER_BAR_MIN_HEIGHT);
        root.set_margin_start(12);
        root.set_margin_end(12);
        root.append(&add);
        root.append(&add_filter);
        root.append(&chips);
        root.append(&count);

        let filter = load_filter(&conn.borrow()).unwrap_or_default();
        let bar = Rc::new(Self {
            root,
            add,
            add_filter,
            chips,
            count,
            chooser,
            choices: RefCell::new(Vec::new()),
            conn,
            filter: RefCell::new(filter),
            visible_count: Cell::new(0),
            total_count: Cell::new(0),
            on_changed: RefCell::new(None),
        });
        wire_chooser(&bar);
        bar.rebuild_chips();
        bar
    }

    pub(super) fn widget(&self) -> &gtk4::Widget {
        self.root.upcast_ref()
    }

    pub(super) fn connect_add(&self, callback: impl Fn() + 'static) {
        self.add.connect_clicked(move |_| callback());
    }

    pub(super) fn set_on_changed(&self, callback: impl Fn(RadioFilter) + 'static) {
        *self.on_changed.borrow_mut() = Some(Rc::new(callback));
    }

    pub(super) fn filter(&self) -> RadioFilter {
        self.filter.borrow().clone()
    }

    pub(super) fn clear_all(self: &Rc<Self>) {
        self.apply(RadioFilter::default());
    }

    pub(super) fn set_rows(&self, rows: &[StationRow]) {
        self.rebuild_choices(rows);
    }

    pub(super) fn set_counts(&self, visible: usize, total: usize) {
        self.visible_count.set(visible);
        self.total_count.set(total);
        self.count.set_text(&if self.filter().is_active() {
            strings::radio_filtered_count(visible, total)
        } else {
            strings::radio_station_count(total)
        });
    }

    fn apply(self: &Rc<Self>, filter: RadioFilter) {
        if let Err(error) = persist_filter(&self.conn.borrow(), &filter) {
            tracing::warn!(%error, "could not persist radio filters");
        }
        self.filter.replace(filter.clone());
        self.rebuild_chips();
        if let Some(callback) = self.on_changed.borrow().clone() {
            callback(filter);
        }
    }

    fn rebuild_choices(&self, rows: &[StationRow]) {
        self.chooser.remove_all();
        let choices = genre_facets(rows)
            .into_iter()
            .map(|value| FilterChoice {
                facet: RadioFilterFacet::Genre,
                value,
            })
            .chain(country_facets(rows).into_iter().map(|value| FilterChoice {
                facet: RadioFilterFacet::Country,
                value,
            }))
            .collect::<Vec<_>>();
        for choice in &choices {
            let row = gtk4::ListBoxRow::new();
            let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
            let facet = gtk4::Label::new(Some(&strings::text(match choice.facet {
                RadioFilterFacet::Genre => strings::RADIO_FILTER_GENRE,
                RadioFilterFacet::Country => strings::RADIO_FILTER_COUNTRY,
            })));
            facet.add_css_class("dim-label");
            let value = gtk4::Label::new(Some(&choice.value));
            value.set_hexpand(true);
            value.set_xalign(0.0);
            content.append(&facet);
            content.append(&value);
            row.set_child(Some(&content));
            self.chooser.append(&row);
        }
        self.choices.replace(choices);
        self.add_filter
            .set_sensitive(!self.choices.borrow().is_empty());
    }

    fn rebuild_chips(self: &Rc<Self>) {
        while let Some(child) = self.chips.first_child() {
            self.chips.remove(&child);
        }
        let filter = self.filter();
        for (facet, value) in [
            (RadioFilterFacet::Genre, filter.genre.as_deref()),
            (RadioFilterFacet::Country, filter.country.as_deref()),
        ] {
            let Some(value) = value else {
                continue;
            };
            let button = gtk4::Button::with_label(value);
            button.add_css_class(CHIP_CSS_CLASS);
            button.set_icon_name("window-close-symbolic");
            let weak = Rc::downgrade(self);
            let current = filter.clone();
            button.connect_clicked(move |_| {
                if let Some(bar) = weak.upgrade() {
                    bar.apply(remove_filter(&current, facet));
                }
            });
            self.chips.append(&button);
        }
        if filter.is_active() {
            let clear = gtk4::Button::with_label(&strings::text(strings::RADIO_CLEAR_ALL));
            clear.add_css_class(CHIP_CSS_CLASS);
            let weak = Rc::downgrade(self);
            clear.connect_clicked(move |_| {
                if let Some(bar) = weak.upgrade() {
                    bar.apply(RadioFilter::default());
                }
            });
            self.chips.append(&clear);
        }
        self.set_counts(self.visible_count.get(), self.total_count.get());
    }
}

fn wire_chooser(bar: &Rc<RadioFilterBar>) {
    let weak = Rc::downgrade(bar);
    bar.chooser.connect_row_activated(move |_, row| {
        let Some(bar) = weak.upgrade() else {
            return;
        };
        let Some(choice) = bar.choices.borrow().get(row.index() as usize).cloned() else {
            return;
        };
        let mut filter = bar.filter();
        match choice.facet {
            RadioFilterFacet::Genre => filter.genre = Some(choice.value),
            RadioFilterFacet::Country => filter.country = Some(choice.value),
        }
        bar.add_filter.popdown();
        bar.apply(filter);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radio_filter_facets_are_sticky_and_each_chip_removes_one_constraint() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        let filter = RadioFilter {
            genre: Some("Metal".into()),
            country: Some("CH".into()),
        };

        persist_filter(&conn, &filter).unwrap();
        assert_eq!(load_filter(&conn).unwrap(), filter);
        assert_eq!(
            remove_filter(&filter, RadioFilterFacet::Genre),
            RadioFilter {
                genre: None,
                country: Some("CH".into()),
            }
        );
        assert_eq!(
            remove_filter(&filter, RadioFilterFacet::Country),
            RadioFilter {
                genre: Some("Metal".into()),
                country: None,
            }
        );
    }

    #[test]
    fn filters_match_case_insensitively_and_facets_are_distinct() {
        let rows = vec![
            test_station(1, Some("Metal"), Some("CH")),
            test_station(2, Some("metal"), Some("DE")),
            test_station(3, Some("Jazz"), Some("CH")),
        ];
        let filtered = filter_rows(
            &rows,
            &RadioFilter {
                genre: Some("METAL".into()),
                country: None,
            },
        );
        assert_eq!(
            filtered.iter().map(|row| row.id).collect::<Vec<_>>(),
            [1, 2]
        );
        assert_eq!(genre_facets(&rows), ["Jazz", "Metal"]);
        assert_eq!(country_facets(&rows), ["CH", "DE"]);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn src_2_add_action_is_tinted_button_not_chip() {
        gtk4::init().unwrap();
        let conn = Rc::new(RefCell::new(
            rusqlite::Connection::open_in_memory().unwrap(),
        ));
        reprise_core::db::migrate(&conn.borrow()).unwrap();
        let bar = RadioFilterBar::new(conn);
        assert!(bar.add.has_css_class(buttons::ADD_ACTION_CLASS));
        assert!(!bar.add.has_css_class(CHIP_CSS_CLASS));
        assert_eq!(bar.root.height_request(), FILTER_BAR_MIN_HEIGHT);
    }

    fn test_station(
        id: i64,
        genre: Option<&str>,
        country: Option<&str>,
    ) -> reprise_core::radio::StationRow {
        reprise_core::radio::StationRow {
            id,
            uuid: None,
            name: format!("Station {id}"),
            stream_url: format!("https://radio.example/{id}"),
            homepage: None,
            favicon_url: None,
            genre: genre.map(str::to_owned),
            codec: None,
            bitrate_kbps: None,
            country_code: country.map(str::to_owned),
            votes: None,
            added_at: 10,
            removed_at: None,
        }
    }
}
