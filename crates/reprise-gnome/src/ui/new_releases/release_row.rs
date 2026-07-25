//! One New Releases list row: cover, title/meta, and a chip<->actions
//! `GtkStack` that reveals a primary action plus Hide on row hover or
//! keyboard focus (NR-10). The primary action either opens the release's
//! announcement externally (NR-11) or, for releases already in the
//! library, navigates to and focuses the album — never a play path (NR-13).

use std::cell::Cell;
use std::rc::Rc;

use chrono::{Datelike, NaiveDate};
use gtk4::prelude::*;

use reprise_core::artist_news::{LibraryPresence, StoredRelease};

use super::release_cover::LazyReleaseCover;
use crate::ui::strings;

/// Compact cover edge shared by every row (NR-9 layout; the old hero/row
/// split is gone — see popover.rs).
const COVER_EDGE: i32 = 40;
const CHIP_CHILD: &str = "chip";
const ACTIONS_CHILD: &str = "actions";

/// Navigates to and focuses an in-library album by (title, artist). Kept as
/// a plain closure type rather than a `MetadataNavigator` reference so this
/// module — and the popover that owns it — stays navigation-agnostic; the
/// window wires the real implementation (NR-13).
pub(in crate::ui) type OnShowAlbum = Rc<dyn Fn(&str, &str)>;

#[derive(Debug, PartialEq, Eq)]
pub(in crate::ui) enum ChipPresentation {
    Upcoming(String),
    Released,
    PartiallyOwned,
    InLibrary,
}

#[derive(Debug, PartialEq, Eq)]
pub(in crate::ui) enum PrimaryAction {
    ShowInLibrary,
    OpenAnnouncement(String),
}

/// `reprise_core::artist_news::parse_partial_date` is `pub(crate)` to that
/// crate, and reprise-gnome is a different crate — so this mirrors its
/// year / year-month / full-date fallback rather than reaching for it.
/// Shared with `history_page.rs` (C1), which needs the same fallback to
/// tell an upcoming history entry from an already-released one.
pub(in crate::ui) fn parse_release_date(value: &str) -> Option<NaiveDate> {
    match value.len() {
        10 => NaiveDate::parse_from_str(value, "%Y-%m-%d").ok(),
        7 => NaiveDate::parse_from_str(&format!("{value}-01"), "%Y-%m-%d").ok(),
        4 => NaiveDate::parse_from_str(&format!("{value}-01-01"), "%Y-%m-%d").ok(),
        _ => None,
    }
}

/// `first_release_date` precision lengths MusicBrainz sends, mirrored from
/// `parse_release_date`'s own fallback chain (full date / year-month / year).
const RELEASE_DATE_FULL_LEN: usize = 10;
const RELEASE_DATE_MONTH_LEN: usize = 7;
const RELEASE_DATE_YEAR_LEN: usize = 4;

/// The meta line's character budget at the popover's ~336px width / 12px
/// type: tuned so `"{artist} · {type} · {date}"` fits the ~260px meta
/// column before the type is dropped in favor of `"{artist} · {date}"`
/// (#1 — the meta line must never ellipsize).
const META_LINE_CHAR_BUDGET: usize = 34;

/// Lokalisiertes Kurzdatum for the meta line (#1). Precision follows the raw
/// string's length, same as `parse_release_date`'s own fallback: a full date
/// renders as "15. Aug", a year-month as "Aug", and a bare year as-is. The
/// year is appended (two digits) only when it differs from `today`'s, so a
/// release from the current year stays as short as possible.
fn format_release_date(raw: &str, today: NaiveDate) -> String {
    let Some(date) = parse_release_date(raw) else {
        return raw.to_string();
    };
    let show_year = date.year() != today.year();
    match raw.len() {
        RELEASE_DATE_FULL_LEN if show_year => date.format("%-d. %b %y").to_string(),
        RELEASE_DATE_FULL_LEN => date.format("%-d. %b").to_string(),
        RELEASE_DATE_MONTH_LEN if show_year => date.format("%b %y").to_string(),
        RELEASE_DATE_MONTH_LEN => date.format("%b").to_string(),
        RELEASE_DATE_YEAR_LEN => date.format("%Y").to_string(),
        _ => raw.to_string(),
    }
}

/// "Artist · Type · Date", dropping the type when the full line would
/// overrun the meta line's character budget (#1) — "Artist · Date" instead
/// of ellipsizing, since the meta line must never truncate with "…".
fn meta_line(artist: &str, release_type: &str, formatted_date: &str) -> String {
    let full = format!("{artist} · {release_type} · {formatted_date}");
    if full.chars().count() <= META_LINE_CHAR_BUDGET {
        full
    } else {
        format!("{artist} · {formatted_date}")
    }
}

fn is_upcoming(release: &StoredRelease, today: NaiveDate) -> bool {
    parse_release_date(&release.first_release_date).is_some_and(|date| date > today)
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
    match release.presence {
        LibraryPresence::Complete => ChipPresentation::InLibrary,
        LibraryPresence::Partial => ChipPresentation::PartiallyOwned,
        LibraryPresence::Absent => ChipPresentation::Released,
    }
}

pub(in crate::ui) fn primary_action(release: &StoredRelease, today: NaiveDate) -> PrimaryAction {
    // Only full ownership navigates into the library. Owning the lead single
    // means the album is still something to go read about, not something to
    // go listen to.
    if release.presence == LibraryPresence::Complete && !is_upcoming(release, today) {
        return PrimaryAction::ShowInLibrary;
    }
    PrimaryAction::OpenAnnouncement(reprise_core::artist_news_links::announce_url_or_fallback(
        release.announce_url.as_deref(),
        &release.release_group_mbid,
    ))
}

/// Whether the actions page should replace the chip. Hover alone would trap
/// keyboard users behind an unreachable reveal, so the stack must also stay
/// on "actions" while focus is anywhere in the row (ACC-1).
pub(in crate::ui) fn stack_target(hovered: bool, focused: bool) -> &'static str {
    if hovered || focused {
        ACTIONS_CHILD
    } else {
        CHIP_CHILD
    }
}

/// Shared with `history_page.rs` (C1) so both rows fall back the same way
/// when the running icon theme lacks a symbolic icon.
pub(in crate::ui) fn icon_with_fallback(
    primary: &'static str,
    fallback: &'static str,
) -> &'static str {
    let Some(display) = gtk4::gdk::Display::default() else {
        return primary;
    };
    if gtk4::IconTheme::for_display(&display).has_icon(primary) {
        primary
    } else {
        fallback
    }
}

/// Shared with `history_page.rs` (C1): both build a flat icon button with a
/// tooltip and an accessible label from the same two arguments.
pub(in crate::ui) fn action_button(icon_name: &str, label: &str) -> gtk4::Button {
    let button = gtk4::Button::from_icon_name(icon_name);
    button.add_css_class("flat");
    button.add_css_class("new-release-action");
    button.set_tooltip_text(Some(label));
    button.update_property(&[gtk4::accessible::Property::Label(label)]);
    button
}

/// Shared with `history_page.rs` (C1): opening an announcement URL is the
/// same "launch externally, log and swallow any failure" action either way.
pub(in crate::ui) fn launch_uri(url: &str) {
    gtk4::UriLauncher::new(url).launch(
        None::<&gtk4::Window>,
        gtk4::gio::Cancellable::NONE,
        |result| {
            if let Err(error) = result {
                tracing::warn!(%error, "could not open announcement URL");
            }
        },
    );
}

fn primary_button(
    release: &StoredRelease,
    today: NaiveDate,
    on_show_album: &OnShowAlbum,
    close_popover: &Rc<dyn Fn()>,
) -> gtk4::Button {
    match primary_action(release, today) {
        PrimaryAction::ShowInLibrary => {
            let icon = icon_with_fallback("go-jump-symbolic", "folder-music-symbolic");
            let button = action_button(icon, &strings::text(strings::SHOW_IN_LIBRARY));
            let close_popover = close_popover.clone();
            let on_show_album = on_show_album.clone();
            let title = release.title.clone();
            let artist = release.artist_name.clone();
            button.connect_clicked(move |_| {
                close_popover();
                on_show_album(&title, &artist);
            });
            button
        }
        PrimaryAction::OpenAnnouncement(url) => {
            let icon = icon_with_fallback("external-link-symbolic", "web-browser-symbolic");
            let button = action_button(icon, &strings::text(strings::OPEN_ANNOUNCEMENT));
            let close_popover = close_popover.clone();
            button.connect_clicked(move |_| {
                close_popover();
                launch_uri(&url);
            });
            button
        }
    }
}

/// Only builds the button; the click is wired in `build`, where the
/// row's `Revealer` exists to collapse before `on_hide` actually runs (B4).
fn hide_button() -> gtk4::Button {
    action_button(
        "view-conceal-symbolic",
        &strings::text(strings::HIDE_RELEASE),
    )
}

fn wire_hover_and_focus(row: &gtk4::Box, stack: &gtk4::Stack) {
    let pointer_inside = Rc::new(Cell::new(false));
    let focus_inside = Rc::new(Cell::new(false));

    let motion = gtk4::EventControllerMotion::new();
    let enter_stack = stack.clone();
    let enter_pointer = pointer_inside.clone();
    let enter_focus = focus_inside.clone();
    motion.connect_enter(move |_, _, _| {
        enter_pointer.set(true);
        enter_stack.set_visible_child_name(stack_target(true, enter_focus.get()));
    });
    let leave_stack = stack.clone();
    let leave_pointer = pointer_inside.clone();
    let leave_focus = focus_inside.clone();
    motion.connect_leave(move |_| {
        leave_pointer.set(false);
        leave_stack.set_visible_child_name(stack_target(false, leave_focus.get()));
    });
    row.add_controller(motion);

    let focus = gtk4::EventControllerFocus::new();
    let focus_stack = stack.clone();
    let focus_pointer = pointer_inside.clone();
    let focus_inside_enter = focus_inside.clone();
    focus.connect_enter(move |_| {
        focus_inside_enter.set(true);
        focus_stack.set_visible_child_name(stack_target(focus_pointer.get(), true));
    });
    let blur_stack = stack.clone();
    focus.connect_leave(move |_| {
        focus_inside.set(false);
        blur_stack.set_visible_child_name(stack_target(pointer_inside.get(), false));
    });
    row.add_controller(focus);
}

/// One popover list entry. Hiding lives on the row itself rather than
/// behind a separate destination, so it stays reachable regardless of list
/// length; "Show in library" navigates and focuses (NR-13) and never plays.
pub(in crate::ui) fn build(
    release: &StoredRelease,
    today: NaiveDate,
    on_hide: &Rc<dyn Fn(&str)>,
    on_show_album: &OnShowAlbum,
    close_popover: &Rc<dyn Fn()>,
) -> gtk4::Widget {
    let cover = LazyReleaseCover::new(
        &release.release_group_mbid,
        &release.artist_name,
        &release.fallback_accent,
        COVER_EDGE,
    );

    let title = gtk4::Label::new(Some(&release.title));
    title.set_xalign(0.0);
    title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    title.add_css_class("new-release-title");

    let formatted_date = format_release_date(&release.first_release_date, today);
    let meta_text = meta_line(&release.artist_name, &release.release_type, &formatted_date);
    let meta = gtk4::Label::new(Some(&meta_text));
    meta.set_xalign(0.0);
    meta.set_ellipsize(gtk4::pango::EllipsizeMode::None);
    meta.add_css_class("new-release-meta");

    let text = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.append(&title);
    text.append(&meta);

    let chip_label = gtk4::Label::new(None);
    chip_label.set_valign(gtk4::Align::Center);
    match chip_presentation(release, today) {
        ChipPresentation::Upcoming(copy) => {
            chip_label.set_label(&copy);
            chip_label.add_css_class("new-release-chip");
        }
        ChipPresentation::Released => {
            chip_label.set_label(&strings::text(strings::RELEASED));
            chip_label.add_css_class("new-release-chip-neutral");
        }
        ChipPresentation::PartiallyOwned => {
            chip_label.set_label(&strings::text(strings::NEW_RELEASES_PARTIALLY_OWNED));
            chip_label.add_css_class("new-release-chip-partial");
        }
        ChipPresentation::InLibrary => {
            chip_label.set_label(&strings::text(strings::IN_LIBRARY));
            chip_label.add_css_class("new-release-chip-neutral");
        }
    }

    let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    actions.set_valign(gtk4::Align::Center);
    actions.append(&primary_button(
        release,
        today,
        on_show_album,
        close_popover,
    ));
    let hide = hide_button();
    actions.append(&hide);

    let right_stack = gtk4::Stack::new();
    right_stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
    right_stack.set_transition_duration(crate::ui::motion::MICRO_MS);
    right_stack.add_named(&chip_label, Some(CHIP_CHILD));
    right_stack.add_named(&actions, Some(ACTIONS_CHILD));
    right_stack.set_visible_child_name(CHIP_CHILD);

    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    row.add_css_class("new-release-row");
    row.append(cover.widget());
    row.append(&text);
    row.append(&right_stack);

    // a11y-semantics: role=group name=new-release-row state=focusable action=tab-into-actions
    row.set_focusable(true);

    wire_hover_and_focus(&row, &right_stack);

    // Hide collapses the row instead of yanking it out (B4): the button only
    // starts the collapse; `on_hide` (which persists hidden/hidden_at and
    // rebuilds the list) runs once the collapse animation is done, i.e. once
    // `child-revealed` goes false. That same notify also fires once the
    // initial `reveal_child(true)` below finishes revealing — the guard here
    // keys off `!is_child_revealed()`, so that initial fire (revealed ==
    // true) never triggers `on_hide`.
    let revealer = gtk4::Revealer::builder()
        .transition_type(gtk4::RevealerTransitionType::SlideUp)
        .transition_duration(crate::ui::motion::STANDARD_MS)
        .child(&row)
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
    hide.connect_clicked(move |_| {
        if let Some(revealer) = revealer_weak.upgrade() {
            revealer.set_reveal_child(false);
        }
    });

    revealer.upcast()
}

#[cfg(test)]
mod tests {
    use super::*;

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
            fallback_accent: "#123456".into(),
            presence: LibraryPresence::Absent,
            announce_url: None,
        }
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 7, 21).unwrap()
    }

    #[test]
    fn format_release_date_full_date_same_year_omits_the_year() {
        assert_eq!(format_release_date("2026-08-15", today()), "15. Aug");
    }

    #[test]
    fn format_release_date_full_date_other_year_appends_two_digit_year() {
        assert_eq!(format_release_date("2025-08-15", today()), "15. Aug 25");
    }

    #[test]
    fn format_release_date_year_month_same_year_is_month_only() {
        assert_eq!(format_release_date("2026-08", today()), "Aug");
    }

    #[test]
    fn format_release_date_year_month_other_year_appends_two_digit_year() {
        assert_eq!(format_release_date("2024-08", today()), "Aug 24");
    }

    #[test]
    fn format_release_date_year_only_renders_the_bare_year() {
        assert_eq!(format_release_date("2026", today()), "2026");
    }

    #[test]
    fn format_release_date_unparsable_falls_back_to_the_raw_value() {
        assert_eq!(format_release_date("tba", today()), "tba");
        assert_eq!(format_release_date("2026-13-40", today()), "2026-13-40");
    }

    #[test]
    fn meta_line_keeps_the_type_when_short_enough() {
        assert_eq!(
            meta_line("Artist", "Album", "15. Aug"),
            "Artist · Album · 15. Aug"
        );
    }

    #[test]
    fn meta_line_drops_the_type_for_a_long_artist_name() {
        let artist = "A Very Long Artist Name That Overruns The Budget";
        assert_eq!(
            meta_line(artist, "Album", "15. Aug"),
            format!("{artist} · 15. Aug")
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
    fn chip_presentation_in_library_when_past_and_in_library() {
        let mut release = release_with_date("2026-01-01");
        release.presence = LibraryPresence::Complete;
        assert_eq!(
            chip_presentation(&release, today()),
            ChipPresentation::InLibrary
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
            ChipPresentation::InLibrary
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
    fn nr_13_in_library_row_offers_show_in_library() {
        let mut release = release_with_date("2026-01-01");
        release.presence = LibraryPresence::Complete;
        assert_eq!(
            primary_action(&release, today()),
            PrimaryAction::ShowInLibrary
        );
    }

    /// In-library releases that have not been released yet must still open
    /// the announcement — "Show in library" would have nothing to reveal.
    #[test]
    fn nr_13_upcoming_in_library_release_still_opens_announcement() {
        let mut release = release_with_date("2026-08-15");
        release.presence = LibraryPresence::Complete;
        assert_eq!(
            primary_action(&release, today()),
            PrimaryAction::OpenAnnouncement(
                "https://musicbrainz.org/release-group/rg-sample".into()
            )
        );
    }

    #[test]
    fn nr_10_stack_target_shows_actions_on_hover_or_focus() {
        assert_eq!(stack_target(false, false), CHIP_CHILD);
        assert_eq!(stack_target(true, false), ACTIONS_CHILD);
        assert_eq!(stack_target(false, true), ACTIONS_CHILD);
        assert_eq!(stack_target(true, true), ACTIONS_CHILD);
    }

    /// Depth-first search rather than a single sibling scan: since B4 wraps
    /// the row in a `Revealer`, the stack is a grandchild (revealer -> row
    /// box -> stack), not a direct sibling of the returned widget.
    fn find_stack(widget: &gtk4::Widget) -> Option<gtk4::Stack> {
        if let Ok(stack) = widget.clone().downcast::<gtk4::Stack>() {
            return Some(stack);
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            if let Some(stack) = find_stack(&current) {
                return Some(stack);
            }
            child = current.next_sibling();
        }
        None
    }

    fn action_buttons(row: &gtk4::Widget) -> Vec<gtk4::Button> {
        let stack = find_stack(row).expect("row exposes a chip/actions stack");
        let actions = stack
            .child_by_name(ACTIONS_CHILD)
            .expect("actions page exists");
        let mut buttons = Vec::new();
        let mut child = actions.first_child();
        while let Some(current) = child {
            if let Ok(button) = current.clone().downcast::<gtk4::Button>() {
                buttons.push(button);
            }
            child = current.next_sibling();
        }
        buttons
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nr_10_row_exposes_both_chip_and_actions_pages() {
        if gtk4::init().is_err() {
            return;
        }
        let release = release_with_date("2026-01-01");
        let on_hide: Rc<dyn Fn(&str)> = Rc::new(|_| {});
        let on_show_album: OnShowAlbum = Rc::new(|_, _| {});
        let close_popover: Rc<dyn Fn()> = Rc::new(|| {});

        let row = build(&release, today(), &on_hide, &on_show_album, &close_popover);

        let stack = find_stack(&row).expect("row exposes a chip/actions stack");
        assert!(stack.child_by_name(CHIP_CHILD).is_some());
        assert!(stack.child_by_name(ACTIONS_CHILD).is_some());
        assert_eq!(stack.visible_child_name().as_deref(), Some(CHIP_CHILD));
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

    /// NR-13: clicking "Show in library" navigates via the injected callback
    /// and closes the popover first. It must never carry a play icon/path.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nr_13_show_in_library_closes_popover_and_navigates_without_play_icon() {
        if gtk4::init().is_err() {
            return;
        }
        let mut release = release_with_date("2026-01-01");
        release.presence = LibraryPresence::Complete;
        let navigated: Rc<std::cell::RefCell<Vec<(String, String)>>> =
            Rc::new(std::cell::RefCell::new(Vec::new()));
        let sink = navigated.clone();
        let on_show_album: OnShowAlbum = Rc::new(move |album: &str, artist: &str| {
            sink.borrow_mut()
                .push((album.to_string(), artist.to_string()));
        });
        let closed = Rc::new(Cell::new(false));
        let close_flag = closed.clone();
        let close_popover: Rc<dyn Fn()> = Rc::new(move || close_flag.set(true));
        let on_hide: Rc<dyn Fn(&str)> = Rc::new(|_| {});

        let row = build(&release, today(), &on_hide, &on_show_album, &close_popover);

        let buttons = action_buttons(&row);
        let primary = buttons
            .first()
            .expect("row exposes a primary action button");
        assert_eq!(primary.icon_name().as_deref(), Some("go-jump-symbolic"));

        primary.emit_clicked();

        assert!(closed.get());
        assert_eq!(
            navigated.borrow().as_slice(),
            [("Release".to_string(), "Artist".to_string())]
        );
    }

    #[test]
    fn partial_ownership_gets_its_own_chip_and_opens_the_announcement() {
        let mut release = release_with_date("2026-01-01");
        release.presence = LibraryPresence::Partial;

        assert_eq!(
            chip_presentation(&release, today()),
            ChipPresentation::PartiallyOwned
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
    fn complete_ownership_still_navigates_into_the_library() {
        let mut release = release_with_date("2026-01-01");
        release.presence = LibraryPresence::Complete;

        assert_eq!(
            chip_presentation(&release, today()),
            ChipPresentation::InLibrary
        );
        assert_eq!(primary_action(&release, today()), PrimaryAction::ShowInLibrary);
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
