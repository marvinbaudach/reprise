//! App-level consented location.
//!
//! `O-4` (2026-07-29, `docs/plans/podcasts-youtube-radio-turn6.md` §8a):
//! Reprise already has exactly one consented location source — city search
//! (Nominatim) or the XDG Location portal, both wired from the Concerts
//! preferences page. That source used to live entirely inside the
//! `concerts.` settings namespace even though it is a general, app-level
//! fact; this module hoists it out so a second feature (Radio, `RAD-5`) can
//! read the same consented value instead of asking the user again or
//! inventing a second source of truth. There are no installations to
//! migrate (`AGENTS.md`), so the settings keys were renamed cleanly rather
//! than kept as fallbacks — `concerts::config::location` now forwards here.

use crate::db::Db;
use rusqlite::Connection;

pub const LOCATION_LAT_KEY: &str = "location.lat";
pub const LOCATION_LON_KEY: &str = "location.lon";
pub const LOCATION_NAME_KEY: &str = "location.name";
/// `RAD-5`: the country code radio-browser filters by. Only ever populated
/// from data a call Reprise already makes — Nominatim's `addressdetails`
/// enrichment of the existing forward-geocode request behind city search —
/// never from a new reverse-geocoding call. A location set via "Use current
/// location" (the XDG portal) carries no country and this stays `None`; see
/// [`AppLocation`].
pub const LOCATION_COUNTRY_KEY: &str = "location.country_code";

/// The one app-level, already-consented location. `latitude`/`longitude`/
/// `name` are always present together once a location is stored;
/// `country_code` is present only when the location came from city search
/// (Nominatim's `addressdetails`) — the portal path ("Use current
/// location") has no textual address at all, so it is honestly `None`
/// rather than guessed.
#[derive(Clone, Debug, PartialEq)]
pub struct AppLocation {
    pub latitude: f64,
    pub longitude: f64,
    pub name: String,
    pub country_code: Option<String>,
}

pub fn app_location(db: &Db) -> Result<Option<AppLocation>, rusqlite::Error> {
    let conn = db.conn();
    app_location_in(conn)
}

pub(crate) fn app_location_in(conn: &Connection) -> Result<Option<AppLocation>, rusqlite::Error> {
    let latitude = numeric_setting(conn, LOCATION_LAT_KEY)?;
    let longitude = numeric_setting(conn, LOCATION_LON_KEY)?;
    let name = non_empty_setting(conn, LOCATION_NAME_KEY)?.unwrap_or_default();
    let country_code = non_empty_setting(conn, LOCATION_COUNTRY_KEY)?;
    Ok(latitude
        .zip(longitude)
        .filter(|(lat, lon)| (-90.0..=90.0).contains(lat) && (-180.0..=180.0).contains(lon))
        .map(|(latitude, longitude)| AppLocation {
            latitude,
            longitude,
            name,
            country_code,
        }))
}

/// Stores a freshly resolved location, replacing whatever was there before —
/// the single write path both Concerts (city search / "Use current
/// location") and any future writer must go through, so there is never a
/// second copy of these keys.
pub fn store(
    db: &Db,
    latitude: f64,
    longitude: f64,
    name: &str,
    country_code: Option<&str>,
) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    crate::library::settings::set_setting_in(conn, LOCATION_LAT_KEY, &latitude.to_string())?;
    crate::library::settings::set_setting_in(conn, LOCATION_LON_KEY, &longitude.to_string())?;
    crate::library::settings::set_setting_in(conn, LOCATION_NAME_KEY, name)?;
    crate::library::settings::set_setting_in(conn, LOCATION_COUNTRY_KEY, country_code.unwrap_or(""))
}

/// Clears the stored location — "Clear" in Concerts preferences. `RAD-5`'s
/// "Near you" chip disappears back to its no-location behavior the moment
/// this runs, because it reads the same keys, not a cached copy.
pub fn clear(db: &Db) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    crate::library::settings::set_setting_in(conn, LOCATION_LAT_KEY, "")?;
    crate::library::settings::set_setting_in(conn, LOCATION_LON_KEY, "")?;
    crate::library::settings::set_setting_in(conn, LOCATION_NAME_KEY, "")?;
    crate::library::settings::set_setting_in(conn, LOCATION_COUNTRY_KEY, "")
}

fn non_empty_setting(conn: &Connection, key: &str) -> Result<Option<String>, rusqlite::Error> {
    Ok(crate::library::settings::get_setting_in(conn, key)?
        .as_deref()
        .and_then(non_empty))
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn numeric_setting(conn: &Connection, key: &str) -> Result<Option<f64>, rusqlite::Error> {
    Ok(non_empty_setting(conn, key)?
        .and_then(|value| value.parse().ok())
        .filter(|value: &f64| value.is_finite()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Db {
        Db::open_in_memory().unwrap()
    }

    #[test]
    fn app_location_round_trips_and_rejects_out_of_range_coordinates() {
        let db = db();
        assert_eq!(app_location(&db).unwrap(), None);

        store(&db, 52.52, 13.405, "Berlin, Deutschland", Some("DE")).unwrap();
        assert_eq!(
            app_location(&db).unwrap(),
            Some(AppLocation {
                latitude: 52.52,
                longitude: 13.405,
                name: "Berlin, Deutschland".into(),
                country_code: Some("DE".into()),
            })
        );

        crate::library::settings::set_setting(&db, LOCATION_LAT_KEY, "999").unwrap();
        assert_eq!(app_location(&db).unwrap(), None);
    }

    #[test]
    fn clear_removes_the_full_location_including_the_country_code() {
        let db = db();
        store(&db, 52.52, 13.405, "Berlin, Deutschland", Some("DE")).unwrap();
        assert!(app_location(&db).unwrap().is_some());

        clear(&db).unwrap();
        assert_eq!(app_location(&db).unwrap(), None);
    }

    #[test]
    fn a_location_without_a_country_code_stores_and_reads_as_none() {
        // The XDG portal path ("Use current location") — no address text,
        // so honestly no country, never a guessed one.
        let db = db();
        store(&db, 47.376, 8.541, "Current location", None).unwrap();
        assert_eq!(
            app_location(&db).unwrap().and_then(|loc| loc.country_code),
            None
        );
    }

    /// `RAD-5`: Radio must read the *same* consented location Concerts
    /// writes, not a copy — this is the substance of `O-4`'s hoist. Set the
    /// location exactly once through the shared `store` and assert that
    /// both `reprise_core::concerts::config::location` (Concerts' reader,
    /// now forwarding here) and `app_location` (Radio's reader) see it.
    /// Deleting the forward in `concerts::config::location` and pointing it
    /// back at a private copy would turn this red.
    #[test]
    fn rad_5_radio_and_concerts_read_the_same_hoisted_location() {
        let db = db();
        store(&db, 52.52, 13.405, "Berlin, Deutschland", Some("DE")).unwrap();

        let for_radio = app_location(&db).unwrap().expect("radio sees a location");
        let for_concerts = crate::concerts::config::location(&db)
            .unwrap()
            .expect("concerts sees a location");

        assert_eq!(for_radio, for_concerts);
        assert_eq!(for_radio.country_code.as_deref(), Some("DE"));
    }
}
