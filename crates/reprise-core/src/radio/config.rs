//! Persisted radio configuration.

use rusqlite::Connection;

use super::search::SearchOrder;

pub const SEARCH_ORDER_KEY: &str = "radio.search_order";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RadioConfig {
    pub search_order: SearchOrder,
}

pub fn load(conn: &Connection) -> Result<RadioConfig, rusqlite::Error> {
    let search_order =
        match crate::library::settings::get_setting(conn, SEARCH_ORDER_KEY)?.as_deref() {
            Some("name") => SearchOrder::Name,
            Some("clicks") => SearchOrder::Clicks,
            _ => SearchOrder::Votes,
        };
    Ok(RadioConfig { search_order })
}

pub fn set_search_order(
    conn: &Connection,
    search_order: SearchOrder,
) -> Result<(), rusqlite::Error> {
    crate::library::settings::set_setting(conn, SEARCH_ORDER_KEY, search_order.setting_value())
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
}
