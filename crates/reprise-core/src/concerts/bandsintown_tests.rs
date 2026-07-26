use super::{artist_url, events_url, parse_artist, parse_events};
use crate::concerts::{ProviderError, Resolution};

#[test]
fn artist_parser_resolves_canonical_name_and_verifies_matching_mbid() {
    let body = r#"{"name":"Lorna Shore","id":510,"mbid":"ABC-def","url":"https://example"}"#;
    assert_eq!(
        parse_artist(body, Some("abc-DEF")).unwrap(),
        Resolution::Resolved {
            provider_id: "Lorna Shore".into(),
            mbid_verified: true,
        }
    );
    assert_eq!(
        parse_artist(body, Some("different")).unwrap(),
        Resolution::Resolved {
            provider_id: "Lorna Shore".into(),
            mbid_verified: false,
        }
    );
}

#[test]
fn urls_encode_artist_names_and_credentials() {
    assert_eq!(
        artist_url("Björk & Friends", "app/id"),
        "https://rest.bandsintown.com/artists/Bj%C3%B6rk%20%26%20Friends?app_id=app%2Fid"
    );
    assert_eq!(
        events_url("Lorna Shore", "key"),
        "https://rest.bandsintown.com/artists/Lorna%20Shore/events?app_id=key"
    );
}

#[test]
fn artist_parser_treats_not_found_payloads_as_unmatched() {
    assert_eq!(
        parse_artist(r#"{"error":"Not Found"}"#, None).unwrap(),
        Resolution::Unmatched
    );
    assert_eq!(parse_artist("{}", None).unwrap(), Resolution::Unmatched);
}

#[test]
fn events_parser_accepts_string_or_number_coordinates_and_offer_fallbacks() {
    let body = r#"[{
      "datetime":"2026-10-17T19:00:00",
      "venue":{"name":"Zenith","city":"München","region":"BY","country":"Germany",
               "latitude":"48.174","longitude":11.555},
      "offers":[{"type":"Tickets","url":"https://tickets.eventim.de/1","status":"available"}],
      "url":"https://www.bandsintown.com/e/1"
    },{
      "datetime":"2026-10-18T20:00:00",
      "venue":{"name":"Backstage","city":"München"},
      "offers":[],
      "url":"https://www.bandsintown.com/e/2"
    }]"#;
    let rows = parse_events(body).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].latitude, Some(48.174));
    assert_eq!(rows[0].longitude, Some(11.555));
    assert_eq!(rows[0].ticket_source.as_deref(), Some("Eventim"));
    assert_eq!(rows[1].ticket_url, None);
    assert_eq!(
        rows[1].event_url.as_deref(),
        Some("https://www.bandsintown.com/e/2")
    );
}

#[test]
fn events_parser_skips_incomplete_rows_and_rejects_invalid_json() {
    let rows =
        parse_events(r#"[{"datetime":"2026-10-17T19:00:00","venue":{"name":"No city"}}]"#).unwrap();
    assert!(rows.is_empty());
    assert_eq!(parse_events("{broken"), Err(ProviderError::Parse));
}
