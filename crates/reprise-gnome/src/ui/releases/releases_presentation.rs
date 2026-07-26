#![allow(dead_code)]

use chrono::NaiveDate;
use reprise_core::artist_news::{release_status, ReleaseStatus};
use reprise_core::artist_news_history::HistoryEntry;

use crate::ui::strings;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ReleasesRowAction {
    Restore,
    ShowInLibrary,
    OpenAnnouncement(String),
}

pub(super) fn format_release_date(raw: &str, _today: NaiveDate) -> String {
    match raw.len() {
        10 => NaiveDate::parse_from_str(raw, "%Y-%m-%d").map_or_else(
            |_| raw.to_string(),
            |date| date.format("%-d %b %y").to_string(),
        ),
        7 => NaiveDate::parse_from_str(&format!("{raw}-01"), "%Y-%m-%d")
            .map_or_else(|_| raw.to_string(), |date| date.format("%b %Y").to_string()),
        4 => NaiveDate::parse_from_str(&format!("{raw}-01-01"), "%Y-%m-%d")
            .map_or_else(|_| raw.to_string(), |date| date.format("%Y").to_string()),
        _ => raw.to_string(),
    }
}

pub(super) fn release_type_label(raw: &str) -> String {
    let message = match raw.trim().to_ascii_lowercase().as_str() {
        "album" => strings::RELEASES_ALBUM,
        "ep" => strings::RELEASES_EP,
        "single" => strings::RELEASES_SINGLE,
        _ => return raw.to_string(),
    };
    strings::text(message)
}

pub(super) fn release_status_label(entry: &HistoryEntry, today: NaiveDate) -> String {
    let message = match release_status(entry, today) {
        ReleaseStatus::InLibrary => strings::RELEASES_IN_LIBRARY,
        ReleaseStatus::Upcoming => strings::RELEASES_UPCOMING,
        ReleaseStatus::Released => strings::RELEASES_RELEASED,
    };
    strings::text(message)
}

pub(super) fn releases_row_action(entry: &HistoryEntry, today: NaiveDate) -> ReleasesRowAction {
    if entry.hidden {
        return ReleasesRowAction::Restore;
    }
    if release_status(entry, today) == ReleaseStatus::InLibrary
        && parse_release_date(&entry.first_release_date).is_none_or(|date| date <= today)
    {
        return ReleasesRowAction::ShowInLibrary;
    }
    ReleasesRowAction::OpenAnnouncement(reprise_core::artist_news_links::announce_url_or_fallback(
        entry.announce_url.as_deref(),
        &entry.release_group_mbid,
    ))
}

fn parse_release_date(raw: &str) -> Option<NaiveDate> {
    match raw.len() {
        10 => NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok(),
        7 => NaiveDate::parse_from_str(&format!("{raw}-01"), "%Y-%m-%d").ok(),
        4 => NaiveDate::parse_from_str(&format!("{raw}-01-01"), "%Y-%m-%d").ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::artist_news::LibraryPresence;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 7, 25).unwrap()
    }

    fn entry(date: &str, presence: LibraryPresence, hidden: bool) -> HistoryEntry {
        HistoryEntry {
            release_group_mbid: "release-id".to_string(),
            artist_name: "Artist".to_string(),
            title: "Release".to_string(),
            release_type: "Album".to_string(),
            first_release_date: date.to_string(),
            first_seen: Some(1),
            seen_at: None,
            hidden,
            hidden_at: hidden.then_some(2),
            presence,
            announce_url: None,
        }
    }

    #[test]
    fn format_release_date_preserves_musicbrainz_precision() {
        assert_eq!(format_release_date("2026-05-29", today()), "29 May 26");
        assert_eq!(format_release_date("2026-05", today()), "May 2026");
        assert_eq!(format_release_date("2026", today()), "2026");
        assert_eq!(format_release_date("unknown", today()), "unknown");
    }

    #[test]
    fn status_pills_follow_query_time_presence_then_date() {
        assert_eq!(
            release_status_label(&entry("2027", LibraryPresence::Complete, false), today()),
            "In library"
        );
        assert_eq!(
            release_status_label(&entry("2026-08", LibraryPresence::Absent, false), today()),
            "upcoming"
        );
        assert_eq!(
            release_status_label(&entry("unknown", LibraryPresence::Partial, false), today()),
            "released"
        );
        assert_eq!(release_type_label("ep"), "EP");
    }

    #[test]
    fn nr_14_activation_uses_restore_library_or_announcement() {
        let hidden = entry("2026-01-01", LibraryPresence::Complete, true);
        assert_eq!(
            releases_row_action(&hidden, today()),
            ReleasesRowAction::Restore
        );

        let owned = entry("2026-01-01", LibraryPresence::Complete, false);
        assert_eq!(
            releases_row_action(&owned, today()),
            ReleasesRowAction::ShowInLibrary
        );

        let upcoming_owned = entry("2026-08-01", LibraryPresence::Complete, false);
        assert_eq!(
            releases_row_action(&upcoming_owned, today()),
            ReleasesRowAction::OpenAnnouncement(
                "https://musicbrainz.org/release-group/release-id".to_string()
            )
        );

        let mut absent = entry("2026-01-01", LibraryPresence::Absent, false);
        absent.announce_url = Some("https://artist.example/release".to_string());
        assert_eq!(
            releases_row_action(&absent, today()),
            ReleasesRowAction::OpenAnnouncement("https://artist.example/release".to_string())
        );
    }
}
