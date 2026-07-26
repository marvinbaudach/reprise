use chrono::NaiveDate;
use serde_json::Value;

use super::{
    http, ArtistRef, EventProvider, ProviderError, ProviderEvent, ProviderKind, Resolution,
};

pub struct TicketmasterProvider {
    api_key: String,
}

impl TicketmasterProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
        }
    }
}

impl EventProvider for TicketmasterProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Ticketmaster
    }

    fn resolve(&self, artist: &ArtistRef<'_>) -> Result<Resolution, ProviderError> {
        if self.api_key.trim().is_empty() {
            return Err(ProviderError::MissingCredentials);
        }
        let body = http::get(&attractions_url(artist.name, &self.api_key))?;
        parse_attractions(&body, artist.name, artist.mbid)
    }

    fn events(&self, provider_id: &str) -> Result<Vec<ProviderEvent>, ProviderError> {
        if self.api_key.trim().is_empty() {
            return Err(ProviderError::MissingCredentials);
        }
        let body = http::get(&events_url(provider_id, &self.api_key))?;
        parse_events(&body)
    }
}

pub(crate) fn attractions_url(name: &str, api_key: &str) -> String {
    format!(
        "https://app.ticketmaster.com/discovery/v2/attractions.json?keyword={}&apikey={}",
        crate::musicbrainz::urlencode(name.trim()),
        crate::musicbrainz::urlencode(api_key.trim())
    )
}

pub(crate) fn events_url(provider_id: &str, api_key: &str) -> String {
    format!(
        "https://app.ticketmaster.com/discovery/v2/events.json?attractionId={}&size=50&apikey={}",
        crate::musicbrainz::urlencode(provider_id.trim()),
        crate::musicbrainz::urlencode(api_key.trim())
    )
}

pub(crate) fn parse_attractions(
    body: &str,
    expected_name: &str,
    expected_mbid: Option<&str>,
) -> Result<Resolution, ProviderError> {
    let value: Value = serde_json::from_str(body).map_err(|_| ProviderError::Parse)?;
    let attractions = value
        .pointer("/_embedded/attractions")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let Some(attraction) = attractions.iter().find(|attraction| {
        attraction
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(|name| name.trim().eq_ignore_ascii_case(expected_name.trim()))
    }) else {
        return Ok(Resolution::Unmatched);
    };
    let Some(provider_id) = attraction.get("id").and_then(non_empty) else {
        return Ok(Resolution::Unmatched);
    };
    let expected_mbid = expected_mbid
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let mbid_verified = expected_mbid.is_some_and(|expected| {
        attraction
            .pointer("/externalLinks/musicbrainz")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|link| link.get("url").and_then(Value::as_str))
            .any(|url| {
                url.to_ascii_lowercase()
                    .contains(&expected.to_ascii_lowercase())
            })
    });
    Ok(Resolution::Resolved {
        provider_id: provider_id.to_owned(),
        mbid_verified,
    })
}

pub(crate) fn parse_events(body: &str) -> Result<Vec<ProviderEvent>, ProviderError> {
    let value: Value = serde_json::from_str(body).map_err(|_| ProviderError::Parse)?;
    let events = value
        .pointer("/_embedded/events")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    Ok(events.iter().filter_map(parse_event).collect())
}

fn parse_event(value: &Value) -> Option<ProviderEvent> {
    let date_key = non_empty(value.pointer("/dates/start/localDate")?)?;
    NaiveDate::parse_from_str(date_key, "%Y-%m-%d").ok()?;
    let local_time = value
        .pointer("/dates/start/localTime")
        .and_then(non_empty)
        .unwrap_or("00:00:00");
    let venue_value = value
        .pointer("/_embedded/venues")
        .and_then(Value::as_array)?
        .first()?;
    let venue = non_empty(venue_value.get("name")?)?;
    let city = non_empty(venue_value.pointer("/city/name")?)?;
    let event_url = value.get("url").and_then(non_empty).map(str::to_owned);
    Some(ProviderEvent {
        provider: ProviderKind::Ticketmaster,
        starts_at: format!("{date_key}T{local_time}"),
        date_key: date_key.to_owned(),
        venue: venue.to_owned(),
        city: city.to_owned(),
        region: venue_value
            .pointer("/state/stateCode")
            .and_then(non_empty)
            .map(str::to_owned),
        country: venue_value
            .pointer("/country/countryCode")
            .and_then(non_empty)
            .or_else(|| venue_value.pointer("/country/name").and_then(non_empty))
            .map(str::to_owned),
        latitude: venue_value
            .pointer("/location/latitude")
            .and_then(parse_number),
        longitude: venue_value
            .pointer("/location/longitude")
            .and_then(parse_number),
        ticket_url: event_url.clone(),
        ticket_source: event_url.as_ref().map(|_| "Ticketmaster".to_owned()),
        event_url,
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
#[path = "ticketmaster_tests.rs"]
mod tests;
