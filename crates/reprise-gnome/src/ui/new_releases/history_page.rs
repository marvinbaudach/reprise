//! History sub-page for the New Releases popover (NR-12): a persistent,
//! timeframe-grouped view of every release ever shown, with hidden entries
//! individually restorable. `popover.rs::show_history` builds this fresh
//! from `query_history` every time the popover navigates to it, rather than
//! keeping one instance alive, so it always reflects the latest snapshot.

use std::rc::Rc;

use chrono::NaiveDate;
use gtk4::prelude::*;

use reprise_core::artist_news_history::{group_history, HistoryEntry};

use super::popover::SCROLLER_MAX_HEIGHT;
use super::release_cover::LazyReleaseCover;
use super::release_row::{self, OnShowAlbum};
use crate::ui::strings;

/// Compact cover edge for a history row (matches `release_row`'s NR-9 rows).
const COVER_EDGE: i32 = 40;
/// The opacity a hidden row renders at: still present and restorable, but
/// visually de-emphasized against unhidden entries.
const HIDDEN_ROW_OPACITY: f64 = 0.55;

#[derive(Debug, PartialEq, Eq)]
pub(in crate::ui) enum HistoryAction {
    Restore,
    ShowInLibrary,
    OpenAnnouncement(String),
}

/// One action per history row: a hidden entry can only be restored — that
/// takes priority even if the release also matches the library, since
/// "restore" is the more specific and more urgent action for a hidden row.
/// A visible entry that matches the library *and* has already been released
/// navigates there; everything else — including an in-library name match
/// whose release date is still ahead of `today` — opens its announcement
/// instead. This mirrors `release_row::primary_action`'s NR-13 carve-out:
/// `in_library` here comes from a name match against the local library (see
/// `query_history`) and can true-positive on an announced reissue/remaster
/// of an album already owned, so "Show in library" must not be offered
/// until the matching release has actually shipped.
pub(in crate::ui) fn history_action(entry: &HistoryEntry, today: NaiveDate) -> HistoryAction {
    if entry.hidden {
        return HistoryAction::Restore;
    }
    if entry.in_library && !is_upcoming(entry, today) {
        return HistoryAction::ShowInLibrary;
    }
    HistoryAction::OpenAnnouncement(reprise_core::artist_news_links::announce_url_or_fallback(
        entry.announce_url.as_deref(),
        &entry.release_group_mbid,
    ))
}

/// Mirrors `release_row::is_upcoming` for `HistoryEntry` (C1): true once the
/// entry's release date parses and lies strictly after `today`.
fn is_upcoming(entry: &HistoryEntry, today: NaiveDate) -> bool {
    release_row::parse_release_date(&entry.first_release_date).is_some_and(|date| date > today)
}

fn row_opacity(entry: &HistoryEntry) -> f64 {
    if entry.hidden {
        HIDDEN_ROW_OPACITY
    } else {
        1.0
    }
}

/// The meta line's status fragment: hidden entries show when they were
/// hidden; released entries show their release date; anything still ahead
/// of `today` shows a countdown — the same three-way split as
/// `release_row::chip_presentation`, rendered as text instead of a chip.
fn status_text(entry: &HistoryEntry, today: NaiveDate) -> String {
    if entry.hidden {
        let date = entry
            .hidden_at
            .map_or_else(String::new, strings::news_timestamp_date);
        return strings::new_releases_hidden_on(&date);
    }
    match release_row::parse_release_date(&entry.first_release_date) {
        Some(date) if date > today => strings::new_releases_days_until((date - today).num_days()),
        _ => strings::new_releases_released_on(&entry.first_release_date),
    }
}

/// Builds the history sub-page content. Returns a plain `gtk4::Widget` so
/// `popover.rs` can drop it straight into its `history_page` container
/// without this module knowing anything about the popover's own layout.
pub(in crate::ui) fn build(
    entries: Vec<HistoryEntry>,
    today: NaiveDate,
    on_back: Rc<dyn Fn()>,
    on_show_album: &OnShowAlbum,
    on_restore: &Rc<dyn Fn(&str)>,
    close_popover: &Rc<dyn Fn()>,
) -> gtk4::Widget {
    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    page.append(&build_header(entries.len(), on_back));

    let list = gtk4::ListBox::new();
    list.set_selection_mode(gtk4::SelectionMode::None);
    for group in group_history(entries, today) {
        list.append(&build_group_header_row(&group.label));
        for entry in &group.entries {
            list.append(&build_entry_row(
                entry,
                today,
                on_show_album,
                on_restore,
                close_popover,
            ));
        }
    }
    let scroller = gtk4::ScrolledWindow::builder()
        .child(&list)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .propagate_natural_height(true)
        .max_content_height(SCROLLER_MAX_HEIGHT)
        .build();
    page.append(&scroller);
    page.append(&build_footer());

    page.upcast()
}

fn build_header(count: usize, on_back: Rc<dyn Fn()>) -> gtk4::Box {
    let back_label = strings::text(strings::NAVIGATE_BACK);
    let back_button = release_row::action_button("go-previous-symbolic", &back_label);
    back_button.connect_clicked(move |_| on_back());

    // #3: uppercase the string itself for consistency with the list page's
    // header (Mockup 2a) — GTK CSS has no text-transform.
    let title = gtk4::Label::new(Some(&strings::text(strings::HISTORY_HEADER).to_uppercase()));
    title.add_css_class("new-release-header");
    title.set_xalign(0.0);
    title.set_hexpand(true);

    let count_pill = gtk4::Label::new(Some(&count.to_string()));
    count_pill.add_css_class("new-release-tag");

    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    header.append(&back_button);
    header.append(&title);
    header.append(&count_pill);
    header
}

/// A group header ("This week", "June", …) as its own non-selectable,
/// non-activatable row, so `GtkListBox`'s `SelectionMode::None` list still
/// visually separates timeframes without either behaving like an entry.
fn build_group_header_row(label: &str) -> gtk4::ListBoxRow {
    let title = gtk4::Label::new(Some(label));
    title.add_css_class("new-release-header");
    title.set_xalign(0.0);

    let row = gtk4::ListBoxRow::new();
    row.set_child(Some(&title));
    row.set_selectable(false);
    row.set_activatable(false);
    row
}

fn build_entry_row(
    entry: &HistoryEntry,
    today: NaiveDate,
    on_show_album: &OnShowAlbum,
    on_restore: &Rc<dyn Fn(&str)>,
    close_popover: &Rc<dyn Fn()>,
) -> gtk4::Widget {
    // History entries carry no stored fallback accent (unlike
    // `StoredRelease`) — an empty string fails `parse_accent` and
    // `LazyReleaseCover` falls back to its own default tile color.
    let cover = LazyReleaseCover::new(
        &entry.release_group_mbid,
        &entry.artist_name,
        "",
        COVER_EDGE,
    );

    let title = gtk4::Label::new(Some(&entry.title));
    title.set_xalign(0.0);
    title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    // #5: shares release_row.rs's title styling so both list types read the
    // same typographic hierarchy.
    title.add_css_class("new-release-title");

    let meta_text = format!("{} · {}", entry.artist_name, status_text(entry, today));
    let meta = gtk4::Label::new(Some(&meta_text));
    meta.set_xalign(0.0);
    meta.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    meta.add_css_class("new-release-meta");

    let text = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.append(&title);
    text.append(&meta);

    let action = build_action_button(entry, today, on_show_album, on_restore, close_popover);

    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    row.add_css_class("new-release-row");
    row.append(cover.widget());
    row.append(&text);
    row.append(&action);

    if entry.hidden {
        row.add_css_class("new-release-hidden");
    }
    row.set_opacity(row_opacity(entry));

    row.upcast()
}

fn build_action_button(
    entry: &HistoryEntry,
    today: NaiveDate,
    on_show_album: &OnShowAlbum,
    on_restore: &Rc<dyn Fn(&str)>,
    close_popover: &Rc<dyn Fn()>,
) -> gtk4::Button {
    match history_action(entry, today) {
        HistoryAction::Restore => {
            let button = release_row::action_button(
                "view-reveal-symbolic",
                &strings::text(strings::SHOW_AGAIN),
            );
            let on_restore = on_restore.clone();
            let mbid = entry.release_group_mbid.clone();
            button.connect_clicked(move |_| on_restore(&mbid));
            button
        }
        HistoryAction::ShowInLibrary => {
            let icon = release_row::icon_with_fallback("go-jump-symbolic", "folder-music-symbolic");
            let button = release_row::action_button(icon, &strings::text(strings::SHOW_IN_LIBRARY));
            let close_popover = close_popover.clone();
            let on_show_album = on_show_album.clone();
            let title = entry.title.clone();
            let artist = entry.artist_name.clone();
            button.connect_clicked(move |_| {
                close_popover();
                on_show_album(&title, &artist);
            });
            button
        }
        HistoryAction::OpenAnnouncement(url) => {
            let icon =
                release_row::icon_with_fallback("external-link-symbolic", "web-browser-symbolic");
            let button =
                release_row::action_button(icon, &strings::text(strings::OPEN_ANNOUNCEMENT));
            let close_popover = close_popover.clone();
            button.connect_clicked(move |_| {
                close_popover();
                release_row::launch_uri(&url);
            });
            button
        }
    }
}

fn build_footer() -> gtk4::Box {
    let check_icon =
        release_row::icon_with_fallback("emblem-ok-symbolic", "object-select-symbolic");
    let caught_up = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    caught_up.append(&gtk4::Image::from_icon_name(check_icon));
    caught_up.append(&gtk4::Label::new(Some(&strings::text(
        strings::ALL_CAUGHT_UP,
    ))));

    let retention = gtk4::Label::new(Some(&strings::text(strings::RETENTION_SIX_MONTHS)));
    retention.add_css_class("dim-label");
    retention.set_hexpand(true);
    retention.set_halign(gtk4::Align::End);

    let footer = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    footer.append(&caught_up);
    footer.append(&retention);
    footer
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(mbid: &str) -> HistoryEntry {
        HistoryEntry {
            release_group_mbid: mbid.to_string(),
            artist_name: "Artist".to_string(),
            title: "Title".to_string(),
            release_type: "Album".to_string(),
            first_release_date: "2026-01-01".to_string(),
            first_seen: Some(1_000),
            seen_at: None,
            hidden: false,
            hidden_at: None,
            in_library: false,
            announce_url: None,
        }
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 7, 21).unwrap()
    }

    /// Mirrors `artist_news_history`'s own `local_timestamp` test helper: a
    /// noon timestamp in local time keeps the fixture stable regardless of
    /// the test runner's timezone or any nearby DST transition.
    fn local_timestamp(date: NaiveDate) -> i64 {
        date.and_hms_opt(12, 0, 0)
            .unwrap()
            .and_local_timezone(chrono::Local)
            .single()
            .unwrap()
            .timestamp()
    }

    #[test]
    fn nr_12_history_action_hidden_entry_is_always_restorable() {
        let mut hidden_and_in_library = entry("one");
        hidden_and_in_library.hidden = true;
        hidden_and_in_library.in_library = true;
        assert_eq!(
            history_action(&hidden_and_in_library, today()),
            HistoryAction::Restore
        );
    }

    #[test]
    fn nr_12_history_action_visible_in_library_entry_shows_in_library() {
        let mut visible_in_library = entry("one");
        visible_in_library.in_library = true;
        assert_eq!(
            history_action(&visible_in_library, today()),
            HistoryAction::ShowInLibrary
        );
    }

    #[test]
    fn nr_12_history_action_visible_entry_opens_its_announcement() {
        let mut with_url = entry("one");
        with_url.announce_url = Some("https://band.example/album".to_string());
        assert_eq!(
            history_action(&with_url, today()),
            HistoryAction::OpenAnnouncement("https://band.example/album".to_string())
        );

        let without_url = entry("two");
        assert_eq!(
            history_action(&without_url, today()),
            HistoryAction::OpenAnnouncement(
                "https://musicbrainz.org/release-group/two".to_string()
            )
        );
    }

    /// NR-13 (C1 carve-out): an in-library history entry whose release date
    /// is still ahead of `today` must open its announcement, not "Show in
    /// library" — mirrors
    /// `release_row::nr_13_upcoming_in_library_release_still_opens_announcement`.
    /// This covers a reissue/remaster/deluxe announcement that name-matches
    /// an album already in the library but has not shipped yet.
    #[test]
    fn nr_13_upcoming_in_library_history_entry_opens_announcement() {
        let mut upcoming_in_library = entry("one");
        upcoming_in_library.in_library = true;
        upcoming_in_library.first_release_date = "2026-08-15".to_string();
        assert_eq!(
            history_action(&upcoming_in_library, today()),
            HistoryAction::OpenAnnouncement(
                "https://musicbrainz.org/release-group/one".to_string()
            )
        );
    }

    #[test]
    fn row_opacity_dims_only_hidden_rows() {
        let visible = entry("one");
        assert_eq!(row_opacity(&visible), 1.0);

        let mut hidden = entry("two");
        hidden.hidden = true;
        assert_eq!(row_opacity(&hidden), HIDDEN_ROW_OPACITY);
    }

    #[test]
    fn status_text_hidden_entry_shows_when_it_was_hidden() {
        let mut hidden = entry("one");
        hidden.hidden = true;
        hidden.hidden_at = Some(0);
        assert!(status_text(&hidden, today()).starts_with("hidden on "));
    }

    #[test]
    fn status_text_released_entry_shows_its_release_date() {
        let mut released = entry("one");
        released.first_release_date = "2026-01-01".to_string();
        assert_eq!(status_text(&released, today()), "released on 2026-01-01");
    }

    #[test]
    fn status_text_upcoming_entry_shows_a_countdown() {
        let mut upcoming = entry("one");
        upcoming.first_release_date = "2026-08-15".to_string();
        assert_eq!(
            status_text(&upcoming, today()),
            strings::new_releases_days_until(25)
        );
    }

    /// Pure mapping test: combines `group_history` with `history_action`
    /// over a fixture spanning "this week" (one hidden, one in-library
    /// entry) and an older month (one announcement entry), asserting group
    /// order and, per row, which action and opacity `build_entry_row` will
    /// look up for it.
    #[test]
    fn nr_12_grouped_entries_map_to_the_expected_action_and_opacity_per_row() {
        let this_week_hidden_at = local_timestamp(today());
        let older_month = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();

        let mut this_week_hidden = entry("this-week-hidden");
        this_week_hidden.first_seen = Some(this_week_hidden_at);
        this_week_hidden.hidden = true;
        this_week_hidden.hidden_at = Some(this_week_hidden_at);

        let mut this_week_in_library = entry("this-week-in-library");
        this_week_in_library.first_seen = Some(this_week_hidden_at);
        this_week_in_library.in_library = true;

        let mut older_announce = entry("older-month-announce");
        older_announce.first_seen = Some(local_timestamp(older_month));

        let groups = group_history(
            vec![this_week_hidden, this_week_in_library, older_announce],
            today(),
        );

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].label, "This week");
        assert_eq!(groups[1].label, "June");

        for row in &groups[0].entries {
            match row.release_group_mbid.as_str() {
                "this-week-hidden" => {
                    assert_eq!(history_action(row, today()), HistoryAction::Restore);
                    assert_eq!(row_opacity(row), HIDDEN_ROW_OPACITY);
                }
                "this-week-in-library" => {
                    assert_eq!(history_action(row, today()), HistoryAction::ShowInLibrary);
                    assert_eq!(row_opacity(row), 1.0);
                }
                other => panic!("unexpected entry in the This week group: {other}"),
            }
        }

        let older_row = &groups[1].entries[0];
        assert_eq!(older_row.release_group_mbid, "older-month-announce");
        assert_eq!(
            history_action(older_row, today()),
            HistoryAction::OpenAnnouncement(
                "https://musicbrainz.org/release-group/older-month-announce".to_string()
            )
        );
        assert_eq!(row_opacity(older_row), 1.0);
    }

    fn find_list_box(widget: &gtk4::Widget) -> Option<gtk4::ListBox> {
        if let Ok(list) = widget.clone().downcast::<gtk4::ListBox>() {
            return Some(list);
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            if let Some(list) = find_list_box(&current) {
                return Some(list);
            }
            child = current.next_sibling();
        }
        None
    }

    fn group_header_label(row_widget: &gtk4::Widget) -> Option<String> {
        let row = row_widget.clone().downcast::<gtk4::ListBoxRow>().ok()?;
        let child = row.child()?;
        let label = child.downcast::<gtk4::Label>().ok()?;
        Some(label.text().to_string())
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn nr_12_history_page_lists_grouped_entries() {
        if gtk4::init().is_err() {
            return;
        }
        let this_week_ts = local_timestamp(today());
        let older_month = NaiveDate::from_ymd_opt(2026, 6, 15).unwrap();

        let mut this_week_entry = entry("this-week");
        this_week_entry.first_seen = Some(this_week_ts);
        let mut older_entry = entry("older");
        older_entry.first_seen = Some(local_timestamp(older_month));

        let on_back: Rc<dyn Fn()> = Rc::new(|| {});
        let on_show_album: OnShowAlbum = Rc::new(|_, _| {});
        let on_restore: Rc<dyn Fn(&str)> = Rc::new(|_| {});
        let close_popover: Rc<dyn Fn()> = Rc::new(|| {});

        let page = build(
            vec![this_week_entry, older_entry],
            today(),
            on_back,
            &on_show_album,
            &on_restore,
            &close_popover,
        );

        let list = find_list_box(&page).expect("history page contains a ListBox");
        let mut header_labels = Vec::new();
        let mut child = list.first_child();
        while let Some(row) = child {
            if let Some(label) = group_header_label(&row) {
                header_labels.push(label);
            }
            child = row.next_sibling();
        }
        assert_eq!(
            header_labels,
            vec!["This week".to_string(), "June".to_string()]
        );
    }
}
