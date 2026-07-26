use chrono::NaiveDate;
use serde_json::Value;

use super::{
    dedupe::ticket_source_label, http, ArtistRef, EventProvider, ProviderError, ProviderEvent,
    ProviderKind, Resolution,
};

pub struct BandsintownProvider {
    app_id: String,
}

impl BandsintownProvider {
    pub fn new(app_id: impl Into<String>) -> Self {
        Self {
            app_id: app_id.into(),
        }
    }
}

impl EventProvider for BandsintownProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Bandsintown
    }

    fn resolve(&self, artist: &ArtistRef<'_>) -> Result<Resolution, ProviderError> {
        if self.app_id.trim().is_empty() {
            return Err(ProviderError::MissingCredentials);
        }
        match http::get(&artist_url(artist.name, &self.app_id)) {
            Ok(body) => parse_artist(&body, artist.mbid),
            Err(ProviderError::HttpStatus(404)) => Ok(Resolution::Unmatched),
            Err(error) => Err(error),
        }
    }

    fn events(&self, provider_id: &str) -> Result<Vec<ProviderEvent>, ProviderError> {
        if self.app_id.trim().is_empty() {
            return Err(ProviderError::MissingCredentials);
        }
        match http::get(&events_url(provider_id, &self.app_id)) {
            Ok(body) => parse_events(&body),
            Err(ProviderError::HttpStatus(404)) => Ok(Vec::new()),
            Err(error) => Err(error),
        }
    }
}

pub(crate) fn artist_url(name: &str, app_id: &str) -> String {
    format!(
        "https://rest.bandsintown.com/artists/{}?app_id={}",
        crate::musicbrainz::urlencode(name.trim()),
        crate::musicbrainz::urlencode(app_id.trim())
    )
}

pub(crate) fn events_url(provider_id: &str, app_id: &str) -> String {
    format!(
        "https://rest.bandsintown.com/artists/{}/events?app_id={}",
        crate::musicbrainz::urlencode(provider_id.trim()),
        crate::musicbrainz::urlencode(app_id.trim())
    )
}

pub(crate) fn parse_artist(
    body: &str,
    expected_mbid: Option<&str>,
) -> Result<Resolution, ProviderError> {
    let value: Value = serde_json::from_str(body).map_err(|_| ProviderError::Parse)?;
    if value.get("error").is_some() {
        return Ok(Resolution::Unmatched);
    }
    let Some(name) = value.get("name").and_then(Value::as_str) else {
        return Ok(Resolution::Unmatched);
    };
    let name = name.trim();
    if name.is_empty() {
        return Ok(Resolution::Unmatched);
    }
    let provider_mbid = value
        .get("mbid")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let mbid_verified = expected_mbid
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .zip(provider_mbid)
        .is_some_and(|(expected, actual)| expected.eq_ignore_ascii_case(actual));
    Ok(Resolution::Resolved {
        provider_id: name.to_owned(),
        mbid_verified,
    })
}

pub(crate) fn parse_events(body: &str) -> Result<Vec<ProviderEvent>, ProviderError> {
    let values: Vec<Value> = serde_json::from_str(body).map_err(|_| ProviderError::Parse)?;
    Ok(values.iter().filter_map(parse_event).collect())
}

fn parse_event(value: &Value) -> Option<ProviderEvent> {
    let starts_at = non_empty(value.get("datetime")?)?;
    let date_key = starts_at.get(..10)?;
    NaiveDate::parse_from_str(date_key, "%Y-%m-%d").ok()?;
    let venue_value = value.get("venue")?;
    let venue = non_empty(venue_value.get("name")?)?;
    let city = non_empty(venue_value.get("city")?)?;
    let ticket_url = value
        .get("offers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|offer| {
            offer
                .get("status")
                .and_then(Value::as_str)
                .is_some_and(|status| status.eq_ignore_ascii_case("available"))
        })
        .and_then(|offer| offer.get("url"))
        .and_then(non_empty)
        .map(str::to_owned);
    Some(ProviderEvent {
        provider: ProviderKind::Bandsintown,
        starts_at: starts_at.to_owned(),
        date_key: date_key.to_owned(),
        venue: venue.to_owned(),
        city: city.to_owned(),
        region: venue_value
            .get("region")
            .and_then(non_empty)
            .map(str::to_owned),
        country: venue_value
            .get("country")
            .and_then(non_empty)
            .map(str::to_owned),
        latitude: venue_value.get("latitude").and_then(parse_number),
        longitude: venue_value.get("longitude").and_then(parse_number),
        ticket_source: ticket_url.as_deref().and_then(ticket_source_label),
        ticket_url,
        event_url: value.get("url").and_then(non_empty).map(str::to_owned),
    })
}

fn non_empty(value: &Value) -> Option<&str> {
    value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn parse_number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.trim().parse().ok())
        .filter(|number| number.is_finite())
}

#[cfg(test)]
#[path = "bandsintown_tests.rs"]
mod tests;
