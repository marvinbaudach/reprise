use chrono::NaiveDate;

use super::{reconcile_artist, LedgerArtist, ProviderEvent, ResolvedIdentity};
use crate::concerts::{ProviderKind, TicketAvailability};

fn listing(venue: &str, ticket_url: &str) -> ProviderEvent {
    ProviderEvent {
        provider: ProviderKind::Ticketmaster,
        availability: TicketAvailability::Unknown,
        starts_at: "2026-10-17T19:00:00".into(),
        date_key: "2026-10-17".into(),
        venue: venue.into(),
        city: "Munich".into(),
        region: Some("BY".into()),
        country: Some("DE".into()),
        latitude: Some(48.17),
        longitude: Some(11.55),
        ticket_url: Some(ticket_url.into()),
        ticket_source: Some("Winner source".into()),
        event_url: Some("https://ticketmaster.com/event/winner".into()),
    }
}

#[test]
fn provider_owned_listing_survives_a_later_losing_batch() {
    let db = crate::db::Db::open_in_memory().unwrap();
    let artist = LedgerArtist {
        key: "artist",
        name: "Artist",
        mbid: None,
        is_similar: false,
        similar_to: None,
    };
    let identity = ResolvedIdentity {
        provider: ProviderKind::Ticketmaster,
        provider_id: "provider-id",
        mbid_verified: false,
    };
    let today = NaiveDate::from_ymd_opt(2026, 7, 25).unwrap();
    let winner = listing("Winner Hall", "https://ticketmaster.com/event/winner");
    reconcile_artist(db.conn(), &artist, &identity, &[winner], today, 1_000).unwrap();

    let mut loser = listing("Loser Hall", "https://reseller.example/event/loser");
    loser.starts_at = "2026-10-17T20:00:00".into();
    loser.availability = TicketAvailability::OffSale;
    loser.latitude = Some(1.0);
    loser.longitude = Some(2.0);
    loser.ticket_source = Some("Loser source".into());
    loser.event_url = Some("https://reseller.example/event/loser".into());
    reconcile_artist(db.conn(), &artist, &identity, &[loser], today, 2_000).unwrap();

    let stored = db
        .conn()
        .query_row(
            "SELECT venue, latitude, longitude, ticket_url, ticket_source,
                    event_url, starts_at, fetched_at, ticket_availability
               FROM concert_events",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<f64>>(1)?,
                    row.get::<_, Option<f64>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, String>(8)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        stored,
        (
            "Winner Hall".into(),
            Some(48.17),
            Some(11.55),
            Some("https://ticketmaster.com/event/winner".into()),
            Some("Winner source".into()),
            Some("https://ticketmaster.com/event/winner".into()),
            "2026-10-17T20:00:00".into(),
            2_000,
            "off_sale".into(),
        )
    );
}
