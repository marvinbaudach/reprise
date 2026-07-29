//! Persisted radio configuration.

use rusqlite::Connection;

use super::search::SearchOrder;

pub const SEARCH_ORDER_KEY: &str = "radio.search_order";
pub const REPORT_PLAYS_KEY: &str = "radio.report_plays";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RadioConfig {
    pub search_order: SearchOrder,
    /// "Report plays to the directory": whether starting a favorite calls
    /// radio-browser.info's click endpoint. Defaults to on, matching the
    /// directory's own play-count etiquette.
    pub report_plays: bool,
}

impl Default for RadioConfig {
    fn default() -> Self {
        Self {
            search_order: SearchOrder::default(),
            report_plays: true,
        }
    }
}

pub fn load(conn: &Connection) -> Result<RadioConfig, rusqlite::Error> {
    let search_order =
        match crate::library::settings::get_setting(conn, SEARCH_ORDER_KEY)?.as_deref() {
            Some("name") => SearchOrder::Name,
            Some("clicks") => SearchOrder::Clicks,
            _ => SearchOrder::Votes,
        };
    let report_plays = crate::library::settings::get_bool(conn, REPORT_PLAYS_KEY, true)?;
    Ok(RadioConfig {
        search_order,
        report_plays,
    })
}

pub fn set_search_order(
    conn: &Connection,
    search_order: SearchOrder,
) -> Result<(), rusqlite::Error> {
    crate::library::settings::set_setting(conn, SEARCH_ORDER_KEY, search_order.setting_value())
}

pub fn set_report_plays(conn: &Connection, value: bool) -> Result<(), rusqlite::Error> {
    crate::library::settings::set_bool(conn, REPORT_PLAYS_KEY, value)
}

/// `NET-1a`: whether a play click may be reported to radio-browser.info
/// right now — ANDs the global online-sources gate, the Radio module, and
/// the "Report plays" preference.
pub fn report_plays_allowed(conn: &Connection) -> Result<bool, rusqlite::Error> {
    Ok(
        crate::online_sources::network_allowed(conn, &crate::modules::RADIO_MODULE)?
            && load(conn)?.report_plays,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_to_votes_and_round_trips_valid_orders() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate(&conn).unwrap();

        assert_eq!(load(&conn).unwrap().search_order, SearchOrder::Votes);
        crate::library::settings::set_setting(&conn, SEARCH_ORDER_KEY, "clicks").unwrap();
        assert_eq!(load(&conn).unwrap().search_order, SearchOrder::Clicks);
    }

    #[test]
    fn config_tolerates_hand_edited_values() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate(&conn).unwrap();
        crate::library::settings::set_setting(&conn, SEARCH_ORDER_KEY, "surprise").unwrap();

        assert_eq!(load(&conn).unwrap().search_order, SearchOrder::Votes);
    }

    #[test]
    fn report_plays_defaults_to_on_and_round_trips() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate(&conn).unwrap();

        assert!(load(&conn).unwrap().report_plays);
        set_report_plays(&conn, false).unwrap();
        assert!(!load(&conn).unwrap().report_plays);
    }

    #[test]
    fn net_1a_report_plays_allowed_ands_global_gate_module_and_preference() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrate(&conn).unwrap();
        // Radio is on by default, the preference is on by default, and the
        // global gate defaults to on — so plays are reported by default.
        assert!(report_plays_allowed(&conn).unwrap());

        set_report_plays(&conn, false).unwrap();
        assert!(!report_plays_allowed(&conn).unwrap());
        set_report_plays(&conn, true).unwrap();

        crate::online_sources::set_enabled(&conn, false).unwrap();
        assert!(
            !report_plays_allowed(&conn).unwrap(),
            "global gate off must block reporting even with Radio and the preference on"
        );
        crate::online_sources::set_enabled(&conn, true).unwrap();

        crate::modules::set_enabled(&conn, &crate::modules::RADIO_MODULE, false).unwrap();
        assert!(!report_plays_allowed(&conn).unwrap());
    }
}
