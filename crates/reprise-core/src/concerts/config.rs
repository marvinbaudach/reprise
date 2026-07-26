use rusqlite::Connection;

use super::{ConcertFilter, DateHorizon};

pub const BANDSINTOWN_APP_ID_KEY: &str = "concerts.bandsintown_app_id";
pub const TICKETMASTER_API_KEY: &str = "concerts.ticketmaster_apikey";
pub const LOCATION_LAT_KEY: &str = "concerts.location_lat";
pub const LOCATION_LON_KEY: &str = "concerts.location_lon";
pub const LOCATION_NAME_KEY: &str = "concerts.location_name";
pub const WINDOW_DAYS_KEY: &str = "concerts.window_days";
pub const DEFAULT_RADIUS_KEY: &str = "concerts.default_radius_km";
pub const SIMILAR_ENABLED_KEY: &str = "concerts.similar_enabled";
pub const SIMILAR_COUNT_KEY: &str = "concerts.similar_count";
pub const FILTER_RADIUS_KEY: &str = "concerts.filter.radius_km";
pub const FILTER_COUNTRY_KEY: &str = "concerts.filter.country";
pub const FILTER_HORIZON_KEY: &str = "concerts.filter.horizon";
pub const FILTER_INCLUDE_SIMILAR_KEY: &str = "concerts.filter.include_similar";
pub const DEFAULT_RADIUS_KM: f64 = 1_000.0;
pub const RADIUS_PRESETS_KM: [u32; 4] = [100, 250, 500, 1_000];

const BANDSINTOWN_ENV: &str = "REPRISE_BANDSINTOWN_APP_ID";
const TICKETMASTER_ENV: &str = "REPRISE_TICKETMASTER_APIKEY";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Credentials {
    pub bandsintown_app_id: Option<String>,
    pub ticketmaster_api_key: Option<String>,
}

impl Credentials {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bandsintown_app_id.is_none() && self.ticketmaster_api_key.is_none()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConcertLocation {
    pub latitude: f64,
    pub longitude: f64,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimilarConfig {
    pub enabled: bool,
    pub count: usize,
}

pub fn credentials(conn: &Connection) -> Result<Credentials, rusqlite::Error> {
    credentials_with_env(conn, |key| std::env::var(key).ok())
}

pub(crate) fn credentials_with_env(
    conn: &Connection,
    read_env: impl Fn(&str) -> Option<String>,
) -> Result<Credentials, rusqlite::Error> {
    Ok(Credentials {
        bandsintown_app_id: non_empty_setting(conn, BANDSINTOWN_APP_ID_KEY)?
            .or_else(|| read_env(BANDSINTOWN_ENV).as_deref().and_then(non_empty)),
        ticketmaster_api_key: non_empty_setting(conn, TICKETMASTER_API_KEY)?
            .or_else(|| read_env(TICKETMASTER_ENV).as_deref().and_then(non_empty)),
    })
}

pub fn location(conn: &Connection) -> Result<Option<ConcertLocation>, rusqlite::Error> {
    let latitude = numeric_setting(conn, LOCATION_LAT_KEY)?;
    let longitude = numeric_setting(conn, LOCATION_LON_KEY)?;
    let name = non_empty_setting(conn, LOCATION_NAME_KEY)?.unwrap_or_default();
    Ok(latitude
        .zip(longitude)
        .filter(|(lat, lon)| (-90.0..=90.0).contains(lat) && (-180.0..=180.0).contains(lon))
        .map(|(latitude, longitude)| ConcertLocation {
            latitude,
            longitude,
            name,
        }))
}

pub fn window_days(conn: &Connection) -> Result<i64, rusqlite::Error> {
    Ok(integer_setting(conn, WINDOW_DAYS_KEY)?
        .unwrap_or(90)
        .clamp(30, 365))
}

pub fn persisted_filter(conn: &Connection) -> Result<ConcertFilter, rusqlite::Error> {
    let stored_radius = crate::library::settings::get_setting(conn, FILTER_RADIUS_KEY)?;
    let radius_km = match stored_radius {
        Some(value) => value.trim().parse::<f64>().ok(),
        None => Some(numeric_setting(conn, DEFAULT_RADIUS_KEY)?.unwrap_or(DEFAULT_RADIUS_KM)),
    }
    .filter(|radius| radius.is_finite() && *radius > 0.0);
    let country = non_empty_setting(conn, FILTER_COUNTRY_KEY)?;
    let horizon = match non_empty_setting(conn, FILTER_HORIZON_KEY)?.as_deref() {
        Some("30" | "next_30_days") => DateHorizon::Next30Days,
        Some("90" | "next_3_months") => DateHorizon::Next3Months,
        Some("180" | "next_6_months") => DateHorizon::Next6Months,
        _ => DateHorizon::AllUpcoming,
    };
    let include_similar =
        crate::library::settings::get_bool(conn, FILTER_INCLUDE_SIMILAR_KEY, false)?;
    Ok(ConcertFilter {
        radius_km,
        country,
        horizon,
        include_similar,
    })
}

pub fn similar_config(conn: &Connection) -> Result<SimilarConfig, rusqlite::Error> {
    let enabled = crate::library::settings::get_bool(conn, SIMILAR_ENABLED_KEY, false)?;
    let count = integer_setting(conn, SIMILAR_COUNT_KEY)?
        .unwrap_or(10)
        .clamp(1, 25) as usize;
    Ok(SimilarConfig { enabled, count })
}

fn non_empty_setting(conn: &Connection, key: &str) -> Result<Option<String>, rusqlite::Error> {
    Ok(crate::library::settings::get_setting(conn, key)?
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

fn integer_setting(conn: &Connection, key: &str) -> Result<Option<i64>, rusqlite::Error> {
    Ok(non_empty_setting(conn, key)?.and_then(|value| value.parse().ok()))
}
