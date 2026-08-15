#![allow(dead_code)]

use std::borrow::Cow;

use chrono::NaiveDate;
use reprise_core::artist_news::{release_status, ReleaseSortKey, ReleaseStatus};
use reprise_core::artist_news_history::HistoryEntry;
use reprise_core::format::DatePattern;
use reprise_view::columns::{ColumnKey, ReleaseColumn};

use crate::ui::strings;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ReleasesRowAction {
    Restore,
    OpenAnnouncement(String),
}

pub(super) fn sort_key_for_id(id: Option<&str>) -> Option<ReleaseSortKey> {
    match id {
        Some(id) if id == ReleaseColumn::Date.as_str() => Some(ReleaseSortKey::Date),
        Some(id) if id == ReleaseColumn::Title.as_str() => Some(ReleaseSortKey::Title),
        Some(id) if id == ReleaseColumn::Artist.as_str() => Some(ReleaseSortKey::Artist),
        Some(id) if id == ReleaseColumn::Type.as_str() => Some(ReleaseSortKey::Type),
        _ => None,
    }
}

/// Renders a MusicBrainz date string at whatever precision it carries, in the
/// system pattern. MusicBrainz supplies `YYYY-MM-DD`, `YYYY-MM` or `YYYY`;
/// anything else is passed through untouched rather than guessed at.
pub(in crate::ui) fn format_partial_date(raw: &str, pattern: &DatePattern) -> String {
    let mut parts = raw.split('-');
    let Some(year) = parts.next().and_then(|value| value.parse::<i32>().ok()) else {
        return raw.to_owned();
    };
    if raw.len() == 4 {
        return pattern.render(Some(year), None, None);
    }
    let month = parts
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|month| (1..=12).contains(month));
    let Some(month) = month else {
        return raw.to_owned();
    };
    if raw.len() == 7 {
        return pattern.render(Some(year), Some(month), None);
    }
    let day = parts
        .next()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|day| (1..=31).contains(day));
    let Some(day) = day else {
        return raw.to_owned();
    };
    if parts.next().is_some() {
        return raw.to_owned();
    }
    pattern.render(Some(year), Some(month), Some(day))
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReleaseLinkLabel {
    Bandcamp,
    MusicBrainz,
    Open,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ReleaseLink<'a> {
    target: Cow<'a, str>,
    label: ReleaseLinkLabel,
}

impl ReleaseLink<'_> {
    pub(super) fn target(&self) -> &str {
        &self.target
    }

    pub(super) fn label(&self) -> String {
        match self.label {
            ReleaseLinkLabel::Bandcamp => strings::text(strings::RELEASES_BANDCAMP),
            ReleaseLinkLabel::MusicBrainz => strings::text(strings::RELEASES_MUSICBRAINZ),
            ReleaseLinkLabel::Open => strings::text(strings::RELEASES_OPEN),
        }
    }
}

pub(super) fn release_link(entry: &HistoryEntry) -> Option<ReleaseLink<'_>> {
    if let Some(target) = bandcamp_purchase_target(entry)
        .filter(|target| reprise_core::external_link::is_launchable_url(target))
    {
        return Some(ReleaseLink {
            target: Cow::Borrowed(target),
            label: ReleaseLinkLabel::Bandcamp,
        });
    }

    if let Some(target) = entry
        .announce_url
        .as_deref()
        .filter(|target| reprise_core::external_link::is_launchable_url(target))
    {
        return Some(ReleaseLink {
            target: Cow::Borrowed(target),
            label: ReleaseLinkLabel::Open,
        });
    }

    let target =
        reprise_core::artist_news_links::announce_url_or_fallback(None, &entry.release_group_mbid);
    reprise_core::external_link::is_launchable_url(&target).then_some(ReleaseLink {
        target: Cow::Owned(target),
        label: ReleaseLinkLabel::MusicBrainz,
    })
}

pub(super) fn release_link_label(entry: &HistoryEntry) -> Option<String> {
    release_link(entry).map(|link| link.label())
}

pub(super) fn release_link_target(entry: &HistoryEntry) -> Option<Cow<'_, str>> {
    release_link(entry).map(|link| link.target)
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
    use reprise_core::format::DatePattern;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 7, 25).unwrap()
    }

    #[test]
    fn sort_key_for_id_maps_the_four_text_columns_and_rejects_cover_status_and_buy() {
        use reprise_core::artist_news::ReleaseSortKey;
        use reprise_view::columns::{ColumnKey, ReleaseColumn};

        assert_eq!(
            sort_key_for_id(Some(ReleaseColumn::Date.as_str())),
            Some(ReleaseSortKey::Date)
        );
        assert_eq!(
            sort_key_for_id(Some(ReleaseColumn::Title.as_str())),
            Some(ReleaseSortKey::Title)
        );
        assert_eq!(
            sort_key_for_id(Some(ReleaseColumn::Artist.as_str())),
            Some(ReleaseSortKey::Artist)
        );
        assert_eq!(
            sort_key_for_id(Some(ReleaseColumn::Type.as_str())),
            Some(ReleaseSortKey::Type)
        );
        for rejected in [
            Some(ReleaseColumn::Cover.as_str()),
            Some(ReleaseColumn::Status.as_str()),
            Some(ReleaseColumn::Buy.as_str()),
            Some("unknown"),
            None,
        ] {
            assert_eq!(sort_key_for_id(rejected), None, "accepted {rejected:?}");
        }
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

    /// STYLE-11: one pattern, all three MusicBrainz precisions, four-digit
    /// year throughout — the reported case where this very column wrote both
    /// `26` and `2026`.
    #[test]
    fn style_11_release_date_keeps_precision_within_one_pattern() {
        let pattern = DatePattern::from_platform("%d.%m.%Y");
        assert_eq!(format_partial_date("2026-05-29", &pattern), "29.05.2026");
        assert_eq!(format_partial_date("2026-05", &pattern), "05.2026");
        assert_eq!(format_partial_date("2026", &pattern), "2026");
        assert_eq!(format_partial_date("unknown", &pattern), "unknown");
        assert_eq!(format_partial_date("2026-13-40", &pattern), "2026-13-40");
    }

    #[test]
    fn status_pills_describe_discography_gaps() {
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
    fn nr_33_activation_uses_restore_or_external_release_link() {
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
    fn nr_30_bandcamp_purchase_target_requires_a_real_bandcamp_relation() {
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

    #[test]
    fn nr_30_release_link_keeps_the_bandcamp_purchase_target() {
        let mut release = entry("2026", LibraryPresence::Absent, false);
        release.announce_url =
            Some("https://oceansleeper.bandcamp.com/album/maybe-death-is-all-i-need".into());

        assert_eq!(release_link_label(&release).as_deref(), Some("Bandcamp"));
        assert_eq!(
            release_link_target(&release).as_deref(),
            Some("https://oceansleeper.bandcamp.com/album/maybe-death-is-all-i-need")
        );
    }

    #[test]
    fn nr_30_release_link_uses_musicbrainz_when_no_announcement_is_stored() {
        let release = entry("2026", LibraryPresence::Absent, false);

        assert_eq!(release_link_label(&release).as_deref(), Some("MusicBrainz"));
        assert_eq!(
            release_link_target(&release).as_deref(),
            Some("https://musicbrainz.org/release-group/release-id")
        );
    }

    #[test]
    fn nr_30_release_link_labels_a_foreign_announcement_as_open() {
        let mut release = entry("2026", LibraryPresence::Absent, false);
        release.announce_url = Some("https://artist.example/releases/new-album".into());

        assert_eq!(release_link_label(&release).as_deref(), Some("Open"));
        assert_eq!(
            release_link_target(&release).as_deref(),
            Some("https://artist.example/releases/new-album")
        );
    }

    #[test]
    fn nr_30_release_link_rejects_non_web_announcements_before_fallback() {
        for announce_url in ["javascript:alert(1)", "file:///etc/passwd"] {
            let mut release = entry("2026", LibraryPresence::Absent, false);
            release.announce_url = Some(announce_url.into());

            assert_eq!(
                release_link_label(&release).as_deref(),
                Some("MusicBrainz"),
                "{announce_url} received the wrong label"
            );
            assert_eq!(
                release_link_target(&release).as_deref(),
                Some("https://musicbrainz.org/release-group/release-id"),
                "{announce_url} escaped instead of using the web fallback"
            );
        }
    }

    #[test]
    fn nr_30_release_link_decision_keeps_label_and_target_together() {
        let mut release = entry("2026", LibraryPresence::Absent, false);
        release.announce_url = Some("https://artist.example/releases/new-album".into());

        let link = release_link(&release).expect("the fallback makes every catalog row linkable");
        assert_eq!(link.label(), "Open");
        assert_eq!(link.target(), "https://artist.example/releases/new-album");
    }
}
