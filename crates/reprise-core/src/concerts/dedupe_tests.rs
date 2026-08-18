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
fn measured_duplicate_pairs_keep_the_provider_owned_ticket_listing() {
    let cases = [
        (
            "Catch Your Breath",
            "2026-11-15",
            "New Haven",
            ("Toads Place - CT", "https://etix.com/event/other"),
            (
                "Toad's Place",
                "https://ticketmaster.com/event/Z7r9jZ1A70U-U",
            ),
            "https://ticketmaster.com/event/Z7r9jZ1A70U-U",
        ),
        (
            "Chelsea Grin",
            "2026-11-28",
            "Chicago",
            ("Riviera Theatre- IL", "https://axs.com/events/other"),
            (
                "Riviera Theatre",
                "https://ticketmaster.com/event/Z7r9jZ1A7P88F",
            ),
            "https://ticketmaster.com/event/Z7r9jZ1A7P88F",
        ),
        (
            "Electric Callboy",
            "2027-02-14",
            "Amsterdam",
            ("Ziggo Dome", "https://ticketmaster.nl/event/vip-upgrades"),
            (
                "Ziggo Dome Club",
                "https://ticketmaster.nl/event/premium-packages",
            ),
            "https://ticketmaster.nl/event/vip-upgrades",
        ),
        (
            "Ocean Sleeper",
            "2026-09-19",
            "Grand Rapids",
            ("The Intersection", "https://etix.com/event/other"),
            (
                "Intersection",
                "https://ticketmaster.com/event/Z7r9jZ1AAZ3xp",
            ),
            "https://ticketmaster.com/event/Z7r9jZ1AAZ3xp",
        ),
        (
            "Wage War",
            "2027-01-15",
            "Cardiff",
            (
                "Y Plas, Cardiff Students Union",
                "https://universe.com/events/other?ref=ticketmaster",
            ),
            (
                "Cardiff University Students Union",
                "https://ticketmaster.co.uk/event/other",
            ),
            "https://ticketmaster.co.uk/event/other",
        ),
    ];

    for (artist, date, city, first, second, expected_url) in cases {
        let mut first_event = event(ProviderKind::Ticketmaster, first.0);
        first_event.date_key = date.into();
        first_event.city = city.into();
        first_event.ticket_url = Some(first.1.into());
        let mut second_event = event(ProviderKind::Ticketmaster, second.0);
        second_event.date_key = date.into();
        second_event.city = city.into();
        second_event.ticket_url = Some(second.1.into());

        let rows = merge(artist, vec![first_event, second_event]);

        assert_eq!(rows.len(), 1, "{artist}");
        assert_eq!(
            rows[0].ticket_url.as_deref(),
            Some(expected_url),
            "{artist}"
        );
    }
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
