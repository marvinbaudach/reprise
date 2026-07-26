use super::{attractions_url, events_url, parse_attractions, parse_events};
use crate::concerts::{ProviderError, Resolution};

#[test]
fn attraction_parser_requires_an_exact_trimmed_case_insensitive_name() {
    let body = r#"{"_embedded":{"attractions":[
      {"id":"wrong","name":"Lorna Shore Tribute"},
      {"id":"right","name":" LORNA SHORE ","externalLinks":{"musicbrainz":[
        {"url":"https://musicbrainz.org/artist/abc-def"}]}}
    ]}}"#;
    assert_eq!(
        parse_attractions(body, "Lorna Shore", Some("ABC-DEF")).unwrap(),
        Resolution::Resolved {
            provider_id: "right".into(),
            mbid_verified: true,
        }
    );
    assert_eq!(
        parse_attractions(body, "Another Artist", None).unwrap(),
        Resolution::Unmatched
    );
}

#[test]
fn urls_encode_search_terms_identifiers_and_credentials() {
    assert_eq!(
        attractions_url("Björk & Friends", "api/key"),
        "https://app.ticketmaster.com/discovery/v2/attractions.json?keyword=Bj%C3%B6rk%20%26%20Friends&apikey=api%2Fkey"
    );
    assert_eq!(
        events_url("artist id", "key"),
        "https://app.ticketmaster.com/discovery/v2/events.json?attractionId=artist%20id&size=50&apikey=key"
    );
}

#[test]
fn event_parser_maps_local_date_time_and_venue() {
    let body = r#"{"_embedded":{"events":[{
      "name":"Lorna Shore",
      "url":"https://ticketmaster.example/e/1",
      "dates":{"start":{"localDate":"2026-10-17","localTime":"19:30:00"}},
      "_embedded":{"venues":[{
        "name":"Zenith","city":{"name":"München"},"state":{"stateCode":"BY"},
        "country":{"name":"Germany","countryCode":"DE"},
        "location":{"latitude":"48.174","longitude":"11.555"}
      }]}
    },{
      "name":"No Time",
      "url":"https://ticketmaster.example/e/2",
      "dates":{"start":{"localDate":"2026-10-18"}},
      "_embedded":{"venues":[{"name":"Backstage","city":{"name":"München"}}]}
    }]}}"#;
    let rows = parse_events(body).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].starts_at, "2026-10-17T19:30:00");
    assert_eq!(rows[0].country.as_deref(), Some("DE"));
    assert_eq!(rows[1].starts_at, "2026-10-18T00:00:00");
    assert_eq!(
        rows[0].ticket_url.as_deref(),
        Some("https://ticketmaster.example/e/1")
    );
}

#[test]
fn event_parser_accepts_an_empty_result_and_rejects_invalid_json() {
    assert!(parse_events("{}").unwrap().is_empty());
    assert_eq!(parse_events("{broken"), Err(ProviderError::Parse));
}
