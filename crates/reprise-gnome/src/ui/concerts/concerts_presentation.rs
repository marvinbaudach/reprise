#![allow(dead_code)]

use chrono::NaiveDate;
use reprise_core::concerts::ConcertRow;
use reprise_core::format::DatePattern;
use reprise_view::columns::{ColumnKey, ConcertColumn};
use std::cmp::Ordering;

use crate::ui::strings;
pub(super) use crate::ui::table_columns::sort::SortDirection;
use crate::ui::table_columns::sort::{self, SortKey, SortSpec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ConcertSortKey {
    Date,
    Distance,
    Artist,
    City,
    Venue,
    Source,
}

/// Unknown ids, including columns from newer builds, leave sorting unchanged.
pub(super) fn sort_key_for_id(id: Option<&str>) -> Option<ConcertSortKey> {
    match id {
        Some(id) if id == ConcertColumn::Date.as_str() => Some(ConcertSortKey::Date),
        Some(id) if id == ConcertColumn::Distance.as_str() => Some(ConcertSortKey::Distance),
        Some(id) if id == ConcertColumn::Artist.as_str() => Some(ConcertSortKey::Artist),
        Some(id) if id == ConcertColumn::City.as_str() => Some(ConcertSortKey::City),
        Some(id) if id == ConcertColumn::Venue.as_str() => Some(ConcertSortKey::Venue),
        Some(id) if id == ConcertColumn::Source.as_str() => Some(ConcertSortKey::Source),
        _ => None,
    }
}

pub(in crate::ui) fn format_event_date(date_key: &str, _today: NaiveDate) -> String {
    format_event_date_with(date_key, &crate::ui::date_format::current().date)
}

/// The pattern-taking form, so the rule can be tested without reaching for
/// the process-wide format.
pub(super) fn format_event_date_with(date_key: &str, pattern: &DatePattern) -> String {
    crate::ui::releases::releases_presentation::format_partial_date(date_key, pattern)
}

pub(super) fn format_distance_km(distance: Option<f64>) -> String {
    distance.map_or_else(
        || "—".to_owned(),
        |distance| format!("{:.0} km", distance.max(0.0).round()),
    )
}

pub(super) fn row_distance(location: Option<(f64, f64)>, event: &ConcertRow) -> Option<f64> {
    let (latitude, longitude) = location?;
    let event_latitude = event.latitude?;
    let event_longitude = event.longitude?;
    Some(reprise_core::concerts::haversine_km(
        latitude,
        longitude,
        event_latitude,
        event_longitude,
    ))
}

pub(super) fn sort_rows(rows: &mut [ConcertRow], key: ConcertSortKey, direction: SortDirection) {
    sort::sort_rows(rows, &SortSpec::new(key, direction));
}

impl SortKey<ConcertRow> for ConcertSortKey {
    fn cmp(&self, left: &ConcertRow, right: &ConcertRow) -> Ordering {
        self.compare(left, right, SortDirection::Ascending)
    }

    fn cmp_descending(&self, left: &ConcertRow, right: &ConcertRow) -> Ordering {
        self.compare(left, right, SortDirection::Descending)
    }
}

impl ConcertSortKey {
    fn compare(self, left: &ConcertRow, right: &ConcertRow, direction: SortDirection) -> Ordering {
        match self {
            ConcertSortKey::Date => sort::compare_optional(
                NaiveDate::parse_from_str(&left.date_key, "%Y-%m-%d").ok(),
                NaiveDate::parse_from_str(&right.date_key, "%Y-%m-%d").ok(),
                direction,
            ),
            ConcertSortKey::Distance => {
                sort::compare_optional(left.distance_km, right.distance_km, direction)
            }
            ConcertSortKey::Artist => {
                sort::compare_text(&left.artist_name, &right.artist_name, direction)
                    .then_with(|| date_tiebreak(left, right))
            }
            ConcertSortKey::City => sort::compare_text(&left.city, &right.city, direction)
                .then_with(|| date_tiebreak(left, right)),
            ConcertSortKey::Venue => sort::compare_text(&left.venue, &right.venue, direction)
                .then_with(|| date_tiebreak(left, right)),
            ConcertSortKey::Source => {
                sort::compare_text(source_name(left), source_name(right), direction)
                    .then_with(|| date_tiebreak(left, right))
            }
        }
    }
}

pub(super) fn source_name(row: &ConcertRow) -> &str {
    row.ticket_source
        .as_deref()
        .filter(|source| !source.trim().is_empty())
        .unwrap_or(row.provider.as_str())
}

#[cfg(test)]
fn present(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

/// Always ascending, independently of `direction`: the tiebreaker provides
/// stability rather than expressing the selected order. Reversing it would
/// make equal-name rows jump twice when the direction changes.
fn date_tiebreak(left: &ConcertRow, right: &ConcertRow) -> Ordering {
    sort::compare_optional(
        NaiveDate::parse_from_str(&left.date_key, "%Y-%m-%d").ok(),
        NaiveDate::parse_from_str(&right.date_key, "%Y-%m-%d").ok(),
        SortDirection::Ascending,
    )
}

pub(super) fn count_line(shown: usize, total: usize) -> String {
    strings::concert_count_line(shown, total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::format::DatePattern;

    fn row(date: &str, distance: Option<f64>) -> ConcertRow {
        ConcertRow {
            id: 1,
            availability: reprise_core::concerts::TicketAvailability::Unknown,
            date_key: date.into(),
            starts_at: format!("{date}T19:00:00"),
            artist_name: "Artist".into(),
            venue: "Venue".into(),
            city: "City".into(),
            region: None,
            country: Some("DE".into()),
            latitude: Some(52.52),
            longitude: Some(13.405),
            distance_km: distance,
            ticket_url: Some("https://ticketmaster.example/1".into()),
            ticket_source: Some("Ticketmaster".into()),
            event_url: Some("https://events.example/1".into()),
            provider: "ticketmaster".into(),
            is_similar: false,
            similar_to: None,
        }
    }

    /// STYLE-11: the weekday and the current-year abbreviation are gone. A
    /// concert date reads exactly like a release date.
    #[test]
    fn style_11_event_date_is_the_system_pattern() {
        let pattern = DatePattern::from_platform("%d.%m.%Y");
        assert_eq!(format_event_date_with("2026-10-17", &pattern), "17.10.2026");
        assert_eq!(format_event_date_with("2027-01-02", &pattern), "02.01.2027");
        assert_eq!(format_event_date_with("broken", &pattern), "broken");
    }

    #[test]
    fn distance_and_count_copy_follow_the_concert_contract() {
        assert_eq!(format_distance_km(Some(417.6)), "418 km");
        assert_eq!(format_distance_km(None), "—");
        assert_eq!(count_line(5, 23), "5 of 23 concerts");
    }

    #[test]
    fn sort_key_for_id_maps_every_sortable_column_and_rejects_the_rest() {
        assert_eq!(
            sort_key_for_id(Some(ConcertColumn::Date.as_str())),
            Some(ConcertSortKey::Date)
        );
        assert_eq!(
            sort_key_for_id(Some(ConcertColumn::Distance.as_str())),
            Some(ConcertSortKey::Distance)
        );
        assert_eq!(
            sort_key_for_id(Some(ConcertColumn::Artist.as_str())),
            Some(ConcertSortKey::Artist)
        );
        assert_eq!(
            sort_key_for_id(Some(ConcertColumn::City.as_str())),
            Some(ConcertSortKey::City)
        );
        assert_eq!(
            sort_key_for_id(Some(ConcertColumn::Venue.as_str())),
            Some(ConcertSortKey::Venue)
        );
        assert_eq!(
            sort_key_for_id(Some(ConcertColumn::Source.as_str())),
            Some(ConcertSortKey::Source)
        );
        assert_eq!(sort_key_for_id(Some(ConcertColumn::Tickets.as_str())), None);
        assert_eq!(sort_key_for_id(Some("future-column")), None);
        assert_eq!(sort_key_for_id(None), None);
    }

    #[test]
    fn artist_sort_is_case_insensitive_and_falls_back_to_the_date() {
        let mut later_alpha = row("2026-10-19", None);
        later_alpha.artist_name = "alpha".into();
        let mut zulu = row("2026-10-16", None);
        zulu.artist_name = "Zulu".into();
        let mut earlier_alpha = row("2026-10-17", None);
        earlier_alpha.artist_name = "alpha".into();
        let mut rows = vec![later_alpha, zulu, earlier_alpha];

        sort_rows(&mut rows, ConcertSortKey::Artist, SortDirection::Ascending);

        assert_eq!(
            rows.iter()
                .map(|row| (row.artist_name.as_str(), row.date_key.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("alpha", "2026-10-17"),
                ("alpha", "2026-10-19"),
                ("Zulu", "2026-10-16"),
            ]
        );
    }

    #[test]
    fn city_and_venue_reverse_with_the_direction_but_keep_the_date_tiebreak_ascending() {
        for key in [ConcertSortKey::City, ConcertSortKey::Venue] {
            let mut later_berlin = row("2026-10-19", None);
            let mut zurich = row("2026-10-18", None);
            let mut earlier_berlin = row("2026-10-17", None);
            match key {
                ConcertSortKey::City => {
                    later_berlin.city = "Berlin".into();
                    zurich.city = "Zurich".into();
                    earlier_berlin.city = "Berlin".into();
                }
                ConcertSortKey::Venue => {
                    later_berlin.venue = "Berlin".into();
                    zurich.venue = "Zurich".into();
                    earlier_berlin.venue = "Berlin".into();
                }
                _ => unreachable!(),
            }
            let mut rows = vec![later_berlin, zurich, earlier_berlin];

            sort_rows(&mut rows, key, SortDirection::Ascending);
            assert_eq!(
                rows.iter()
                    .map(|row| row.date_key.as_str())
                    .collect::<Vec<_>>(),
                vec!["2026-10-17", "2026-10-19", "2026-10-18"]
            );

            sort_rows(&mut rows, key, SortDirection::Descending);
            assert_eq!(
                rows.iter()
                    .map(|row| row.date_key.as_str())
                    .collect::<Vec<_>>(),
                vec!["2026-10-18", "2026-10-17", "2026-10-19"]
            );
        }
    }

    #[test]
    fn source_sorts_by_the_displayed_name_not_the_raw_field() {
        let mut provider_fallback = row("2026-10-17", None);
        provider_fallback.ticket_source = None;
        provider_fallback.provider = "ticketmaster".into();
        let mut zulu = row("2026-10-18", None);
        zulu.ticket_source = Some("Zulu".into());
        zulu.provider = "aaa".into();
        let mut alpha = row("2026-10-19", None);
        alpha.ticket_source = Some("Alpha".into());
        alpha.provider = "zzz".into();
        let mut rows = vec![provider_fallback, zulu, alpha];

        sort_rows(&mut rows, ConcertSortKey::Source, SortDirection::Ascending);

        assert_eq!(
            rows.iter().map(source_name).collect::<Vec<_>>(),
            vec!["Alpha", "ticketmaster", "Zulu"]
        );
    }

    #[test]
    fn a_blank_text_field_sorts_last_in_both_directions() {
        fn text_for(row: &ConcertRow, key: ConcertSortKey) -> &str {
            match key {
                ConcertSortKey::Artist => &row.artist_name,
                ConcertSortKey::City => &row.city,
                ConcertSortKey::Venue => &row.venue,
                ConcertSortKey::Source => source_name(row),
                _ => unreachable!(),
            }
        }

        fn rows_for(key: ConcertSortKey) -> Vec<ConcertRow> {
            ["", "Zulu", "   ", "Alpha"]
                .into_iter()
                .enumerate()
                .map(|(index, value)| {
                    let mut row = row(&format!("2026-10-{}", 17 + index), None);
                    match key {
                        ConcertSortKey::Artist => row.artist_name = value.into(),
                        ConcertSortKey::City => row.city = value.into(),
                        ConcertSortKey::Venue => row.venue = value.into(),
                        ConcertSortKey::Source => {
                            row.ticket_source = Some(value.into());
                            row.provider.clear();
                        }
                        _ => unreachable!(),
                    }
                    row
                })
                .collect()
        }

        for key in [
            ConcertSortKey::Artist,
            ConcertSortKey::City,
            ConcertSortKey::Venue,
            ConcertSortKey::Source,
        ] {
            for (direction, expected_present) in [
                (SortDirection::Ascending, ["Alpha", "Zulu"]),
                (SortDirection::Descending, ["Zulu", "Alpha"]),
            ] {
                let mut rows = rows_for(key);
                sort_rows(&mut rows, key, direction);
                assert_eq!(
                    rows[..2]
                        .iter()
                        .map(|row| text_for(row, key))
                        .collect::<Vec<_>>(),
                    expected_present
                );
                assert!(rows[2..]
                    .iter()
                    .all(|row| present(text_for(row, key)).is_none()));
            }
        }
    }

    #[test]
    fn distance_uses_haversine_only_with_two_complete_locations() {
        let event = row("2026-10-17", None);
        let distance = row_distance(Some((48.137, 11.575)), &event).unwrap();
        assert!((distance - 504.0).abs() < 6.0);
        assert_eq!(row_distance(None, &event), None);
    }

    #[test]
    fn sort_keeps_missing_distances_at_the_end_in_both_directions() {
        let mut rows = vec![
            row("2026-10-18", None),
            row("2026-10-17", Some(400.0)),
            row("2026-10-19", Some(100.0)),
        ];
        sort_rows(
            &mut rows,
            ConcertSortKey::Distance,
            SortDirection::Ascending,
        );
        assert_eq!(
            rows.iter().map(|row| row.distance_km).collect::<Vec<_>>(),
            vec![Some(100.0), Some(400.0), None]
        );
        sort_rows(
            &mut rows,
            ConcertSortKey::Distance,
            SortDirection::Descending,
        );
        assert_eq!(
            rows.iter().map(|row| row.distance_km).collect::<Vec<_>>(),
            vec![Some(400.0), Some(100.0), None]
        );
    }

    #[test]
    fn date_sort_defaults_to_chronological_order_and_invalid_dates_end_last() {
        let mut rows = vec![
            row("broken", None),
            row("2026-10-19", None),
            row("2026-10-17", None),
        ];
        sort_rows(&mut rows, ConcertSortKey::Date, SortDirection::Ascending);
        assert_eq!(
            rows.iter()
                .map(|row| row.date_key.as_str())
                .collect::<Vec<_>>(),
            vec!["2026-10-17", "2026-10-19", "broken"]
        );
    }
}
