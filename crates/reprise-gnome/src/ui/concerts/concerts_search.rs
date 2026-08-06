//! FIL-1d: the Concerts section's query, and nothing else.
//!
//! Its own module so `concerts_view` stays under the repository's
//! 800-line source-size gate, and so the one statement that decides
//! which columns the chip may claim lives by itself.

use reprise_core::concerts::ConcertRow;

/// FIL-1d: "in artist and venue" — no other column takes part, so the chip's
/// promise and the match stay one statement.
pub(super) fn concerts_matching(rows: Vec<ConcertRow>, query: &str) -> Vec<ConcertRow> {
    if query.trim().is_empty() {
        return rows;
    }
    rows.into_iter()
        .filter(|row| {
            reprise_view::search_scope::matches_any(
                [row.artist_name.as_str(), row.venue.as_str()],
                query,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn concert(artist: &str, venue: &str) -> ConcertRow {
        ConcertRow {
            id: 0,
            date_key: "2026-08-05".into(),
            starts_at: "2026-08-05T20:00:00Z".into(),
            artist_name: artist.to_owned(),
            venue: venue.to_owned(),
            city: "Köln".into(),
            region: None,
            country: None,
            latitude: None,
            longitude: None,
            distance_km: None,
            ticket_url: None,
            ticket_source: None,
            event_url: None,
            provider: "test".into(),
            is_similar: false,
            similar_to: None,
        }
    }

    /// UX FIL-1d: the Concerts query matches **artist and venue** — the two
    /// fields its chip names — case-insensitively and mid-word. The city is
    /// deliberately not searched, because the chip does not claim it.
    #[test]
    fn fil_1d_concerts_query_matches_artist_and_venue_only() {
        let rows = vec![
            concert("Lorna Shore", "Palladium"),
            concert("Quiet Hands", "Antwerpen Hall"),
            concert("Elsewhere", "Live Music Hall"),
        ];

        let artists = |query: &str| {
            concerts_matching(rows.clone(), query)
                .into_iter()
                .map(|row| row.artist_name)
                .collect::<Vec<_>>()
        };

        assert_eq!(artists("wer"), ["Quiet Hands"]);
        assert_eq!(artists("LORNA"), ["Lorna Shore"]);
        assert_eq!(artists("palladium"), ["Lorna Shore"]);
        assert!(
            artists("köln").is_empty(),
            "the city is not one of the two fields the chip names"
        );
        assert_eq!(artists("").len(), 3);
    }
}
