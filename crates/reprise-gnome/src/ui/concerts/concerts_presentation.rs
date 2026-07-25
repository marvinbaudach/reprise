#![allow(dead_code)]

use chrono::{Datelike, NaiveDate};
use reprise_core::concerts::ConcertRow;
use std::cmp::Ordering;

use crate::ui::strings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ConcertSortKey {
    Date,
    Distance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SortDirection {
    Ascending,
    Descending,
}

pub(super) fn format_event_date(date_key: &str, today: NaiveDate) -> String {
    let Ok(date) = NaiveDate::parse_from_str(date_key, "%Y-%m-%d") else {
        return date_key.to_owned();
    };
    if date.year() == today.year() {
        date.format("%a, %b %-d").to_string()
    } else {
        date.format("%a, %b %-d, %Y").to_string()
    }
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
    rows.sort_by(|left, right| match key {
        ConcertSortKey::Date => compare_optional(
            NaiveDate::parse_from_str(&left.date_key, "%Y-%m-%d").ok(),
            NaiveDate::parse_from_str(&right.date_key, "%Y-%m-%d").ok(),
            direction,
        ),
        ConcertSortKey::Distance => {
            compare_optional(left.distance_km, right.distance_km, direction)
        }
    });
}

fn compare_optional<T: PartialOrd>(
    left: Option<T>,
    right: Option<T>,
    direction: SortDirection,
) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => {
            let ordering = left.partial_cmp(&right).unwrap_or(Ordering::Equal);
            match direction {
                SortDirection::Ascending => ordering,
                SortDirection::Descending => ordering.reverse(),
            }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

pub(super) fn count_line(shown: usize, total: usize) -> String {
    strings::concert_count_line(shown, total)
}

pub(super) fn ticket_button_label(row: &ConcertRow) -> Option<String> {
    row.ticket_url.as_ref().or(row.event_url.as_ref()).map(|_| {
        row.ticket_source
            .as_deref()
            .filter(|source| !source.trim().is_empty())
            .map_or_else(|| strings::text(strings::CONCERTS_TICKETS), strings::text)
    })
}

pub(super) fn updated_ago(latest_attempt: Option<i64>, now: i64) -> String {
    latest_attempt.map_or_else(
        || strings::text(strings::CONCERTS_UPDATED_NEVER),
        |timestamp| strings::concerts_updated_ago(timestamp, now),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(date: &str, distance: Option<f64>) -> ConcertRow {
        ConcertRow {
            id: 1,
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

    #[test]
    fn event_date_is_compact_and_adds_a_year_only_when_needed() {
        let today = NaiveDate::from_ymd_opt(2026, 7, 25).unwrap();
        assert_eq!(format_event_date("2026-10-17", today), "Sat, Oct 17");
        assert_eq!(format_event_date("2027-01-02", today), "Sat, Jan 2, 2027");
        assert_eq!(format_event_date("broken", today), "broken");
    }

    #[test]
    fn distance_and_count_copy_follow_the_concert_contract() {
        assert_eq!(format_distance_km(Some(417.6)), "418 km");
        assert_eq!(format_distance_km(None), "—");
        assert_eq!(count_line(5, 23), "5 of 23 concerts");
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

    #[test]
    fn tickets_and_updated_copy_degrade_without_optional_values() {
        let mut event = row("2026-10-17", None);
        assert_eq!(ticket_button_label(&event).as_deref(), Some("Ticketmaster"));
        event.ticket_source = None;
        assert_eq!(ticket_button_label(&event).as_deref(), Some("Tickets"));
        event.ticket_url = None;
        event.event_url = None;
        assert_eq!(ticket_button_label(&event), None);
        assert_eq!(updated_ago(None, 10_000), "Never updated");
        assert_eq!(updated_ago(Some(9_900), 10_000), "Updated 1 min ago");
    }
}
