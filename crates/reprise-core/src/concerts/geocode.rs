use serde_json::Value;

use super::{http, ProviderError};

#[derive(Clone, Debug, PartialEq)]
pub struct GeocodedLocation {
    pub lat: f64,
    pub lon: f64,
    pub city: String,
    pub country: Option<String>,
    /// `RAD-5`/`O-4`: the ISO 3166-1 alpha-2 country code, uppercased to
    /// match radio-browser's convention, when Nominatim's `addressdetails`
    /// enrichment of this same request returned one. Never derived from a
    /// separate reverse-geocoding call.
    pub country_code: Option<String>,
}

#[must_use]
pub fn geocode_url(query: &str, language: Option<&str>) -> String {
    let mut url = format!(
        "https://nominatim.openstreetmap.org/search?q={}&format=json&limit=1&addressdetails=1",
        crate::musicbrainz::urlencode(query.trim())
    );
    if let Some(language) = language.map(str::trim).filter(|value| !value.is_empty()) {
        let language = language.replace('_', "-");
        url.push_str("&accept-language=");
        url.push_str(&crate::musicbrainz::urlencode(&language));
    }
    url
}

pub fn geocode(
    query: &str,
    language: Option<&str>,
) -> Result<Option<GeocodedLocation>, ProviderError> {
    let body = http::get(&geocode_url(query, language))?;
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
    let Some(display_name) = non_empty_text(value.get("display_name")) else {
        return Ok(None);
    };
    let address = value.get("address");
    let city = ["city", "town", "village", "municipality"]
        .into_iter()
        .find_map(|key| non_empty_text(address.and_then(|address| address.get(key))))
        .or_else(|| {
            display_name
                .split(',')
                .next()
                .map(str::trim)
                .filter(|city| !city.is_empty())
        });
    let Some(city) = city else {
        return Ok(None);
    };
    let country =
        non_empty_text(address.and_then(|address| address.get("country"))).map(str::to_owned);
    let country_code = non_empty_text(address.and_then(|address| address.get("country_code")))
        .map(str::to_ascii_uppercase);
    Ok(Some(GeocodedLocation {
        lat,
        lon,
        city: city.to_owned(),
        country,
        country_code,
    }))
}

fn non_empty_text(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn parse_number(value: Option<&Value>) -> Option<f64> {
    value?
        .as_f64()
        .or_else(|| value?.as_str()?.trim().parse().ok())
        .filter(|number| number.is_finite())
}
