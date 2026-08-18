use super::{dedupe_key, merge, normalize_component, ticket_source_label};
use crate::concerts::{ProviderEvent, ProviderKind, TicketAvailability};

fn event(provider: ProviderKind, venue: &str) -> ProviderEvent {
    ProviderEvent {
        provider,
        availability: TicketAvailability::Unknown,
        starts_at: "2026-10-17T19:00:00".into(),
        date_key: "2026-10-17".into(),
        venue: venue.into(),
        city: " München ".into(),
        region: None,
        country: Some("DE".into()),
        latitude: None,
        longitude: None,
        ticket_url: None,
        ticket_source: None,
        event_url: None,
    }
}

#[test]
fn normalized_dedupe_key_folds_case_diacritics_and_whitespace() {
    assert_eq!(normalize_component("  MÜNCHEN   Süd  "), "munchen sud");
    assert_eq!(
        dedupe_key("  BJÖRK  ", "2026-10-17", " München "),
        "bjork|2026-10-17|munchen"
    );
}

#[test]
fn merge_collapses_venues_and_prefers_bandsintown() {
    let rows = merge(
        "Lorna Shore",
        vec![
            event(ProviderKind::Ticketmaster, "Zénith"),
            event(ProviderKind::Bandsintown, "ZENITH"),
            event(ProviderKind::Ticketmaster, "Backstage"),
        ],
    );
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].provider, ProviderKind::Bandsintown);
    assert_eq!(rows[0].venue, "ZENITH");
}

#[test]
fn merge_collapses_two_venues_for_the_same_artist_date_and_city() {
    let rows = merge(
        "Lorna Shore",
        vec![
            event(ProviderKind::Ticketmaster, "Matinee Hall"),
            event(ProviderKind::Ticketmaster, "Evening Hall"),
        ],
    );

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].venue, "Matinee Hall");
}

#[test]
fn ticket_source_uses_known_domains_and_a_readable_fallback() {
    assert_eq!(
        ticket_source_label("https://tickets.eventim.de/show"),
        Some("Eventim".into())
    );
    assert_eq!(
        ticket_source_label("https://www.ticketmaster.ch/event/1"),
        Some("Ticketmaster".into())
    );
    assert_eq!(
        ticket_source_label("https://tickets.rockhouse.example/path"),
        Some("Rockhouse".into())
    );
    assert_eq!(ticket_source_label("not a url"), None);
}
