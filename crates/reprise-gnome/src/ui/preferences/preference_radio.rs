//! Radio plugin preferences.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::radio::search::SearchOrder;
use rusqlite::Connection;

use crate::ui::strings;

#[derive(Clone)]
pub(in crate::ui) struct RadioPreferenceRows {
    rows: Rc<Vec<gtk4::Widget>>,
}

impl RadioPreferenceRows {
    pub(in crate::ui) fn add_to(&self, group: &adw::PreferencesGroup) {
        for row in self.rows.iter() {
            group.add(row);
        }
    }

    pub(in crate::ui) fn set_sensitive(&self, enabled: bool) {
        for row in self.rows.iter() {
            row.set_sensitive(enabled);
        }
    }
}

pub(in crate::ui) fn build(conn: &Rc<RefCell<Connection>>, enabled: bool) -> RadioPreferenceRows {
    let config = reprise_core::radio::config::load(&conn.borrow()).unwrap_or_default();
    let model = gtk4::StringList::new(&[
        &strings::text(strings::RADIO_ORDER_VOTES),
        &strings::text(strings::RADIO_ORDER_NAME),
        &strings::text(strings::RADIO_ORDER_CLICKS),
    ]);
    let order = adw::ComboRow::builder()
        .title(strings::text(strings::RADIO_SEARCH_ORDER))
        .model(&model)
        .selected(order_index(config.search_order))
        .build();
    {
        let conn = conn.clone();
        order.connect_selected_notify(move |row| {
            if let Err(error) = save_search_order(&conn.borrow(), order_for_index(row.selected())) {
                tracing::warn!(%error, "could not save radio preference");
            }
        });
    }

    let report_plays = adw::SwitchRow::builder()
        .title(strings::text(strings::RADIO_REPORT_PLAYS))
        .active(config.report_plays)
        .build();
    {
        let conn = conn.clone();
        report_plays.connect_active_notify(move |row| {
            if let Err(error) = save_report_plays(&conn.borrow(), row.is_active()) {
                tracing::warn!(%error, "could not save radio preference");
            }
        });
    }

    let rows = RadioPreferenceRows {
        rows: Rc::new(vec![order.upcast(), report_plays.upcast()]),
    };
    rows.set_sensitive(enabled);
    rows
}

fn order_index(order: SearchOrder) -> u32 {
    match order {
        SearchOrder::Votes => 0,
        SearchOrder::Name => 1,
        SearchOrder::Clicks => 2,
    }
}

fn order_for_index(index: u32) -> SearchOrder {
    match index {
        1 => SearchOrder::Name,
        2 => SearchOrder::Clicks,
        _ => SearchOrder::Votes,
    }
}

fn save_search_order(conn: &Connection, value: SearchOrder) -> Result<(), rusqlite::Error> {
    reprise_core::radio::config::set_search_order(conn, value)
}

fn save_report_plays(conn: &Connection, value: bool) -> Result<(), rusqlite::Error> {
    reprise_core::radio::config::set_report_plays(conn, value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radio_search_order_preference_round_trips() {
        let conn = reprise_core::db::open_migrated(None).unwrap();
        save_search_order(&conn, reprise_core::radio::search::SearchOrder::Clicks).unwrap();
        assert_eq!(
            reprise_core::radio::config::load(&conn)
                .unwrap()
                .search_order,
            reprise_core::radio::search::SearchOrder::Clicks
        );
    }

    #[test]
    fn report_plays_preference_round_trips() {
        let conn = reprise_core::db::open_migrated(None).unwrap();
        save_report_plays(&conn, false).unwrap();
        assert!(
            !reprise_core::radio::config::load(&conn)
                .unwrap()
                .report_plays
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn radio_preference_rows_build_with_search_order_and_report_plays() {
        gtk4::init().unwrap();
        let conn = Rc::new(RefCell::new(reprise_core::db::open_migrated(None).unwrap()));
        let rows = build(&conn, true);
        assert_eq!(rows.rows.len(), 2);
    }
}
