#![allow(dead_code)]

use chrono::NaiveDate;
use reprise_core::artist_news::{release_status, ReleaseStatus};
use reprise_core::artist_news_history::HistoryEntry;

use crate::ui::strings;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ReleasesRowAction {
    Restore,
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
    match release_status(entry, today) {
        ReleaseStatus::InLibrary => strings::text(strings::RELEASES_IN_LIBRARY),
        ReleaseStatus::Upcoming => strings::text(strings::RELEASES_UPCOMING),
        ReleaseStatus::Incomplete => entry.track_count.map_or_else(
            || strings::text(strings::RELEASES_INCOMPLETE),
            |track_count| strings::release_track_count_line(entry.local_track_count, track_count),
        ),
        ReleaseStatus::Missing => strings::text(strings::RELEASES_MISSING),
    }
}

pub(super) fn bandcamp_purchase_target(entry: &HistoryEntry) -> Option<&str> {
    reprise_core::artist_news_links::bandcamp_purchase_url(entry.announce_url.as_deref())
}

pub(super) fn releases_row_action(entry: &HistoryEntry, _today: NaiveDate) -> ReleasesRowAction {
    if entry.hidden {
        return ReleasesRowAction::Restore;
    }
    ReleasesRowAction::OpenAnnouncement(reprise_core::artist_news_links::announce_url_or_fallback(
        entry.announce_url.as_deref(),
        &entry.release_group_mbid,
    ))
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
            track_count: None,
            local_track_count: 0,
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
    fn nr_17_status_pills_describe_discography_gaps() {
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
            "Incomplete"
        );
        let mut partial = entry("2026-01-01", LibraryPresence::Partial, false);
        partial.local_track_count = 1;
        partial.track_count = Some(5);
        assert_eq!(release_status_label(&partial, today()), "1 of 5 tracks");
        assert_eq!(
            release_status_label(
                &entry("2026-01-01", LibraryPresence::Absent, false),
                today()
            ),
            "Missing"
        );
        assert_eq!(release_type_label("ep"), "EP");
    }

    #[test]
    fn nr_17_activation_uses_restore_or_external_release_link() {
        let hidden = entry("2026-01-01", LibraryPresence::Complete, true);
        assert_eq!(
            releases_row_action(&hidden, today()),
            ReleasesRowAction::Restore
        );

        let upcoming = entry("2026-08-01", LibraryPresence::Absent, false);
        assert_eq!(
            releases_row_action(&upcoming, today()),
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

    #[test]
    fn nr_20_bandcamp_purchase_target_requires_a_real_bandcamp_relation() {
        let mut release = entry("2026", LibraryPresence::Absent, false);
        release.announce_url =
            Some("https://oceansleeper.bandcamp.com/album/maybe-death-is-all-i-need".into());
        assert_eq!(
            bandcamp_purchase_target(&release),
            Some("https://oceansleeper.bandcamp.com/album/maybe-death-is-all-i-need")
        );

        release.announce_url = Some("https://musicbrainz.org/release-group/release-id".into());
        assert_eq!(bandcamp_purchase_target(&release), None);

        release.announce_url = Some("https://bandcamp.com.evil.example/album/fake".into());
        assert_eq!(bandcamp_purchase_target(&release), None);
    }
}
