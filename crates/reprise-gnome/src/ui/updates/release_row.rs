//! One New Releases row: a 44px cover, title/meta, a persistent date tag,
//! one flat activation surface, and a sibling Hide button. Activation always
//! follows NR-11's announcement URL priority, independent of library presence.

use std::rc::Rc;

use chrono::NaiveDate;
use gtk4::prelude::*;

use reprise_core::artist_news::StoredRelease;

use super::feed_row;
use super::release_cover::LazyReleaseCover;
use crate::ui::releases::releases_presentation::format_partial_date;
use crate::ui::strings;

/// Compact cover edge shared by every row (NR-9 layout; the old hero/row
/// split is gone — see popover.rs).
const COVER_EDGE: i32 = 44;

/// Retained in the constructor seam until the owning window wiring can remove
/// the now-obsolete album-navigation callback without crossing strand ownership.
pub(in crate::ui) type OnShowAlbum = Rc<dyn Fn(&str, &str)>;

#[derive(Debug, PartialEq, Eq)]
pub(in crate::ui) enum ChipPresentation {
    Upcoming(String),
    Released,
}

#[derive(Debug, PartialEq, Eq)]
pub(in crate::ui) enum PrimaryAction {
    OpenAnnouncement(String),
}

/// `reprise_core::artist_news::parse_partial_date` is `pub(crate)` to that
/// crate, and reprise-gnome is a different crate — so this mirrors its
/// year / year-month / full-date fallback rather than reaching for it.
/// Kept local to this module because the core date parser is crate-private.
pub(in crate::ui) fn parse_release_date(value: &str) -> Option<NaiveDate> {
    match value.len() {
        10 => NaiveDate::parse_from_str(value, "%Y-%m-%d").ok(),
        7 => NaiveDate::parse_from_str(&format!("{value}-01"), "%Y-%m-%d").ok(),
        4 => NaiveDate::parse_from_str(&format!("{value}-01-01"), "%Y-%m-%d").ok(),
        _ => None,
    }
}

fn meta_line(artist: &str, release_type: &str, formatted_date: &str) -> String {
    strings::updates_release_meta(artist, release_type, formatted_date)
}

pub(in crate::ui) fn chip_presentation(
    release: &StoredRelease,
    today: NaiveDate,
) -> ChipPresentation {
    if let Some(date) = parse_release_date(&release.first_release_date) {
        if date > today {
            let days_until = (date - today).num_days();
            return ChipPresentation::Upcoming(strings::new_releases_days_until(days_until));
        }
    }
    ChipPresentation::Released
}

pub(in crate::ui) fn primary_action(release: &StoredRelease, _today: NaiveDate) -> PrimaryAction {
    PrimaryAction::OpenAnnouncement(reprise_core::artist_news_links::announce_url_or_fallback(
        release.announce_url.as_deref(),
        &release.release_group_mbid,
    ))
}

pub(in crate::ui) fn launch_uri(url: &str) {
    crate::ui::external_link::launch(url, "announcement", None);
}

fn release_source(url: &str) -> String {
    if reprise_core::artist_news_links::bandcamp_purchase_url(Some(url)).is_some() {
        return strings::text(strings::RELEASES_BANDCAMP);
    }
    if url.starts_with("https://musicbrainz.org/") || url.starts_with("http://musicbrainz.org/") {
        return strings::text(strings::RELEASES_MUSICBRAINZ);
    }
    url.split("//")
        .nth(1)
        .and_then(|tail| tail.split('/').next())
        .filter(|host| !host.trim().is_empty())
        .map_or_else(
            || strings::text(strings::RELEASES_OPEN),
            |host| host.trim_start_matches("www.").to_owned(),
        )
}

/// One popover list entry. Hiding lives on the row itself rather than
/// behind a separate destination, so it stays reachable regardless of list
/// length.
pub(in crate::ui) fn build(
    release: &StoredRelease,
    today: NaiveDate,
    on_hide: &Rc<dyn Fn(&str)>,
    _on_show_album: &OnShowAlbum,
    close_popover: &Rc<dyn Fn()>,
) -> gtk4::Widget {
    let cover = LazyReleaseCover::new(
        &release.release_group_mbid,
        &release.artist_name,
        COVER_EDGE,
    );

    let formatted_date = format_partial_date(
        &release.first_release_date,
        &crate::ui::date_format::current().date,
    );
    let meta_text = meta_line(&release.artist_name, &release.release_type, &formatted_date);
    let action = primary_action(release, today);
    let (tooltip, on_activate): (String, Rc<dyn Fn()>) = match action {
        PrimaryAction::OpenAnnouncement(url) => {
            let tooltip = strings::updates_opens_source(&release_source(&url));
            let close_popover = close_popover.clone();
            (
                tooltip,
                Rc::new(move || {
                    close_popover();
                    launch_uri(&url);
                }),
            )
        }
    };
    let tag = match chip_presentation(release, today) {
        ChipPresentation::Upcoming(copy) => feed_row::Tag {
            text: copy,
            tone: feed_row::TagTone::Neutral,
        },
        ChipPresentation::Released => feed_row::Tag {
            text: strings::text(strings::UPDATES_RELEASED),
            tone: feed_row::TagTone::Accent,
        },
    };

    // Hide collapses the row instead of yanking it out (B4): the button only
    // starts the collapse; `on_hide` (which persists hidden/hidden_at and
    // rebuilds the list) runs once the collapse animation is done, i.e. once
    // `child-revealed` goes false. That same notify also fires once the
    // initial `reveal_child(true)` below finishes revealing — the guard here
    // keys off `!is_child_revealed()`, so that initial fire (revealed ==
    // true) never triggers `on_hide`.
    let feed_row = feed_row::build(
        cover.widget(),
        feed_row::Presentation {
            title: release.title.clone(),
            title_suffix: None,
            meta: meta_text,
            tag: Some(tag),
            tooltip,
            activatable: true,
        },
        &strings::text(strings::HIDE_RELEASE),
        on_activate,
        Rc::new(|| {}),
    );
    let dismiss = feed_row.dismiss.clone();
    let revealer = gtk4::Revealer::builder()
        .transition_type(gtk4::RevealerTransitionType::SlideUp)
        .transition_duration(crate::ui::motion::STANDARD_MS)
        .child(&feed_row.root)
        .reveal_child(true)
        .build();

    let on_hide = on_hide.clone();
    let mbid = release.release_group_mbid.clone();
    revealer.connect_child_revealed_notify(move |rev| {
        if !rev.is_child_revealed() {
            on_hide(&mbid);
        }
    });

    // Weak: the button (owned by the row, owned by the revealer) must not
    // hold a strong ref back to the revealer, or the pair leaks.
    let revealer_weak = revealer.downgrade();
    dismiss.connect_clicked(move |_| {
        if let Some(revealer) = revealer_weak.upgrade() {
            revealer.set_reveal_child(false);
        }
    });

    revealer.upcast()
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::artist_news::LibraryPresence;
    use reprise_core::format::DatePattern;
    use std::cell::Cell;

    fn release_with_date(date: &str) -> StoredRelease {
        StoredRelease {
            release_group_mbid: "rg-sample".into(),
            artist_name: "Artist".into(),
            artist_mbid: "artist-id".into(),
            title: "Release".into(),
            release_type: "Album".into(),
            first_release_date: date.into(),
            fetched_at: 100,
            seen_at: None,
            hidden: false,
            presence: LibraryPresence::Absent,
            announce_url: None,
            track_count: None,
            local_track_count: 0,
        }
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 7, 21).unwrap()
    }

    /// STYLE-11: the popover used to drop the year inside the current year
    /// and write it two-digit otherwise. Both are gone; it renders exactly
    /// what the table renders.
    #[test]
    fn style_11_popover_release_date_matches_the_table() {
        let pattern = DatePattern::from_platform("%d.%m.%Y");
        assert_eq!(format_partial_date("2026-08-15", &pattern), "15.08.2026");
        assert_eq!(format_partial_date("2025-08-15", &pattern), "15.08.2025");
        assert_eq!(format_partial_date("2026-08", &pattern), "08.2026");
        assert_eq!(format_partial_date("2026", &pattern), "2026");
        assert_eq!(format_partial_date("tba", &pattern), "tba");
    }

    #[test]
    fn meta_line_keeps_the_type_when_short_enough() {
        assert_eq!(
            meta_line("Artist", "Album", "15. Aug"),
            "Artist · Album · 15. Aug"
        );
    }

    #[test]
    fn meta_line_keeps_all_fields_for_a_long_artist_name() {
        let artist = "A Very Long Artist Name That Overruns The Budget";
        assert_eq!(
            meta_line(artist, "Album", "15. Aug"),
            format!("{artist} · Album · 15. Aug")
        );
    }

    #[test]
    fn chip_presentation_upcoming_shows_days_until() {
        let release = release_with_date("2026-08-15");
        assert_eq!(
            chip_presentation(&release, today()),
            ChipPresentation::Upcoming(strings::new_releases_days_until(25))
        );
    }

    #[test]
    fn chip_presentation_released_when_past_and_not_in_library() {
        let release = release_with_date("2026-01-01");
        assert_eq!(
            chip_presentation(&release, today()),
            ChipPresentation::Released
        );
    }

    #[test]
    fn chip_presentation_released_when_past_and_in_library() {
        let mut release = release_with_date("2026-01-01");
        release.presence = LibraryPresence::Complete;
        assert_eq!(
            chip_presentation(&release, today()),
            ChipPresentation::Released
        );
    }

    /// A partial (year-only) date must parse via the same fallback the core
    /// crate uses, rather than fail closed as if there were no date.
    #[test]
    fn chip_presentation_handles_partial_date() {
        let mut release = release_with_date("2024");
        release.presence = LibraryPresence::Complete;
        assert_eq!(
            chip_presentation(&release, today()),
            ChipPresentation::Released
        );
    }

    #[test]
    fn nr_11_row_opens_announce_url_or_fallback() {
        let mut release = release_with_date("2026-01-01");
        release.announce_url = Some("https://band.example/album".into());
        assert_eq!(
            primary_action(&release, today()),
            PrimaryAction::OpenAnnouncement("https://band.example/album".into())
        );

        release.announce_url = None;
        assert_eq!(
            primary_action(&release, today()),
            PrimaryAction::OpenAnnouncement(
                "https://musicbrainz.org/release-group/rg-sample".into()
            )
        );
    }

    #[test]
    fn nr_38_in_library_row_still_opens_the_announcement_url() {
        let mut release = release_with_date("2026-01-01");
        release.presence = LibraryPresence::Complete;
        assert_eq!(
            primary_action(&release, today()),
            PrimaryAction::OpenAnnouncement(
                "https://musicbrainz.org/release-group/rg-sample".into()
            )
        );
    }

    /// In-library releases that have not been released yet must still open
    /// the announcement — "Show in library" would have nothing to reveal.
    #[test]
    fn upcoming_in_library_release_still_opens_announcement() {
        let mut release = release_with_date("2026-08-15");
        release.presence = LibraryPresence::Complete;
        assert_eq!(
            primary_action(&release, today()),
            PrimaryAction::OpenAnnouncement(
                "https://musicbrainz.org/release-group/rg-sample".into()
            )
        );
    }

    fn action_buttons(row: &gtk4::Widget) -> Vec<gtk4::Button> {
        let mut buttons = Vec::new();
        if let Ok(button) = row.clone().downcast::<gtk4::Button>() {
            if button.has_css_class("new-release-action") {
                buttons.push(button);
            }
        }
        let mut child = row.first_child();
        while let Some(current) = child {
            buttons.extend(action_buttons(&current));
            child = current.next_sibling();
        }
        buttons
    }

    /// B4: the Hide button no longer invokes `on_hide` directly — it only
    /// starts the Revealer's collapse (`set_reveal_child(false)`); `on_hide`
    /// (persist + rebuild) is wired to the `child-revealed` notify instead.
    /// This test never realizes/maps the row (no window is shown — see the
    /// brief), and GTK's `Revealer` skips the timed tween entirely for an
    /// unmapped widget, jumping straight to the target position — so both
    /// the initial `reveal_child(true)` and the click-triggered
    /// `reveal_child(false)` resolve synchronously here, which is what lets
    /// this test assert the guard deterministically without pumping the
    /// main loop: `on_hide` must stay unfired across construction (the
    /// initial reveal notifies with `is_child_revealed() == true`, which the
    /// `if !revealed` guard must swallow) and only fire once, after the
    /// click actually collapses the row.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn hide_button_collapses_the_row_before_removing_it() {
        if gtk4::init().is_err() {
            return;
        }
        let release = release_with_date("2026-01-01");
        let hidden = Rc::new(Cell::new(false));
        let sink = hidden.clone();
        let on_hide: Rc<dyn Fn(&str)> = Rc::new(move |_: &str| sink.set(true));
        let on_show_album: OnShowAlbum = Rc::new(|_, _| {});
        let close_popover: Rc<dyn Fn()> = Rc::new(|| {});

        let row = build(&release, today(), &on_hide, &on_show_album, &close_popover);
        let revealer = row
            .clone()
            .downcast::<gtk4::Revealer>()
            .expect("build wraps the row in a Revealer (B4)");
        assert!(revealer.reveals_child(), "row starts revealed");
        assert!(
            !hidden.get(),
            "the initial reveal(true) must not trigger on_hide (init guard)"
        );

        let buttons = action_buttons(&row);
        let hide = buttons.last().expect("row exposes a Hide button");
        hide.emit_clicked();

        assert!(!revealer.reveals_child(), "click collapses the row");
        assert!(
            hidden.get(),
            "on_hide must run once the collapse finishes (child-revealed == false)"
        );
    }

    #[test]
    fn partial_ownership_keeps_the_date_tag_and_opens_the_announcement() {
        let mut release = release_with_date("2026-01-01");
        release.presence = LibraryPresence::Partial;

        assert_eq!(
            chip_presentation(&release, today()),
            ChipPresentation::Released
        );
        assert!(
            matches!(
                primary_action(&release, today()),
                PrimaryAction::OpenAnnouncement(_)
            ),
            "owning one track means you want the rest, not a jump into the library"
        );
    }

    #[test]
    fn complete_ownership_still_opens_the_announcement() {
        let mut release = release_with_date("2026-01-01");
        release.presence = LibraryPresence::Complete;

        assert_eq!(
            chip_presentation(&release, today()),
            ChipPresentation::Released
        );
        assert_eq!(
            primary_action(&release, today()),
            PrimaryAction::OpenAnnouncement(
                "https://musicbrainz.org/release-group/rg-sample".into()
            )
        );
    }

    #[test]
    fn upcoming_still_outranks_every_presence_state() {
        for presence in [
            LibraryPresence::Absent,
            LibraryPresence::Partial,
            LibraryPresence::Complete,
        ] {
            let mut release = release_with_date("2026-08-15");
            release.presence = presence;
            assert!(matches!(
                chip_presentation(&release, today()),
                ChipPresentation::Upcoming(_)
            ));
            assert!(matches!(
                primary_action(&release, today()),
                PrimaryAction::OpenAnnouncement(_)
            ));
        }
    }
}
