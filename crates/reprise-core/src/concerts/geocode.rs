use serde_json::Value;

use super::{http, ProviderError};

#[derive(Clone, Debug, PartialEq)]
pub struct GeocodedLocation {
    pub lat: f64,
    pub lon: f64,
    pub display_name: String,
}

#[must_use]
pub fn geocode_url(query: &str) -> String {
    format!(
        "https://nominatim.openstreetmap.org/search?q={}&format=json&limit=1",
        crate::musicbrainz::urlencode(query.trim())
    )
}

pub fn geocode(query: &str) -> Result<Option<GeocodedLocation>, ProviderError> {
    let body = http::get(&geocode_url(query))?;
    parse_geocode(&body)
}

pub fn parse_geocode(body: &str) -> Result<Option<GeocodedLocation>, ProviderError> {
    let values: Vec<Value> = serde_json::from_str(body).map_err(|_| ProviderError::Parse)?;
    let Some(value) = values.first() else {
        return Ok(None);
    };
    let Some(lat) = parse_number(value.get("lat")) else {
        return Ok(None);
    };
    let Some(lon) = parse_number(value.get("lon")) else {
        return Ok(None);
    };
    let Some(display_name) = value
        .get("display_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    Ok(Some(GeocodedLocation {
        lat,
        lon,
        display_name: display_name.to_owned(),
    }))
}

fn parse_number(value: Option<&Value>) -> Option<f64> {
    value?
        .as_f64()
        .or_else(|| value?.as_str()?.trim().parse().ok())
        .filter(|number| number.is_finite())
}
